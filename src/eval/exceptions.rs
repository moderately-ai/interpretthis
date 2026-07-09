// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use rustpython_parser::ast;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{eval_body, eval_expr},
    state::InterpreterState,
    tools::Tools,
    value::{ExceptionValue, Value},
};

/// Evaluate a try/except/finally block.
///
/// Key semantics:
/// - Signals (Break, Continue, Return, `FinalAnswer`) are NOT caught by except handlers.
/// - Only `EvalError::Exception` and `EvalError::Interpreter` are caught.
/// - Finally blocks ALWAYS run, even on signals.
/// - An exception in finally overrides any previous exception.
pub async fn eval_try(
    state: &mut InterpreterState,
    node: &ast::StmtTry,
    tools: &Tools,
) -> EvalResult {
    let mut result = Value::None;
    let mut pending_error: Option<EvalError> = None;

    // Execute try body
    match eval_body(state, &node.body, tools).await {
        Ok(val) => {
            result = val;
            // Try succeeded — run else clause
            match eval_body(state, &node.orelse, tools).await {
                Ok(val) => result = val,
                Err(e) => pending_error = Some(e),
            }
        }
        Err(EvalError::Signal(sig)) => {
            // Signals propagate through try/except — NOT caught
            // But finally still runs
            pending_error = Some(EvalError::Signal(sig));
        }
        Err(EvalError::Exception(exc)) => {
            if let Some((value, new_error)) =
                try_match_handlers(state, &exc, &node.handlers, tools).await?
            {
                result = value;
                pending_error = new_error;
            } else {
                // No handler matched — exception propagates after finally.
                pending_error = Some(EvalError::Exception(exc));
            }
        }
        Err(EvalError::Interpreter(ie)) => {
            // Interpreter errors can be caught by `except`; convert to an
            // `ExceptionValue` so matches_handler can inspect the type.
            let exc = interpreter_error_to_exception(&ie);
            if let Some((value, new_error)) =
                try_match_handlers(state, &exc, &node.handlers, tools).await?
            {
                result = value;
                pending_error = new_error;
            } else {
                pending_error = Some(EvalError::Interpreter(ie));
            }
        }
    }

    // Always execute finally block
    if !node.finalbody.is_empty() {
        match eval_body(state, &node.finalbody, tools).await {
            Ok(_) => {
                // Finally succeeded; propagate pending error if any
            }
            Err(finally_err) => {
                // Exception in finally overrides any previous exception
                pending_error = Some(finally_err);
            }
        }
    }

    // Propagate pending error
    if let Some(err) = pending_error {
        return Err(err);
    }

    Ok(result)
}

/// Run the first matching handler in `handlers` against `exc`, binding the
/// handler variable, executing the body, and unbinding it after. Returns
/// `None` when no handler matches (so the caller propagates the original
/// error), or `Some((value, new_error))` when a handler ran: `new_error`
/// carries a signal or fresh exception raised inside the handler body.
async fn try_match_handlers(
    state: &mut InterpreterState,
    exc: &ExceptionValue,
    handlers: &[ast::ExceptHandler],
    tools: &Tools,
) -> Result<Option<(Value, Option<EvalError>)>, EvalError> {
    for handler in handlers {
        let ast::ExceptHandler::ExceptHandler(h) = handler;
        if !matches_handler(state, exc, h, tools).await? {
            continue;
        }

        if let Some(ref name) = h.name {
            state
                .set_variable(name.as_str(), Value::Exception(exc.clone()))
                .map_err(EvalError::Interpreter)?;
        }

        // Push the caught exception onto the active stack so a bare
        // `raise` inside the handler re-raises it, and a fresh raise
        // inherits it as implicit `__context__`. Popped in every exit
        // path below to keep the stack symmetric.
        state.active_exception_stack.push(exc.clone());

        let body_result = eval_body(state, &h.body, tools).await;

        state.active_exception_stack.pop();

        let (value, new_error) = match body_result {
            Ok(val) => (val, None),
            // Any error from the handler body — signal or new exception —
            // replaces the original exception and is returned for the try
            // to surface after finally runs.
            Err(err) => (Value::None, Some(err)),
        };

        if let Some(ref name) = h.name {
            let _ = state.delete_variable(name.as_str());
        }

        return Ok(Some((value, new_error)));
    }
    Ok(None)
}

/// Check if an exception matches an except handler.
async fn matches_handler(
    state: &mut InterpreterState,
    exc: &ExceptionValue,
    handler: &ast::ExceptHandlerExceptHandler,
    tools: &Tools,
) -> Result<bool, EvalError> {
    // Bare except: catches everything
    let Some(type_expr) = &handler.type_ else {
        return Ok(true);
    };

    // Evaluate the exception type expression
    let type_val = eval_expr(state, type_expr, tools).await?;

    // Handle tuple of exception types: except (ValueError, TypeError)
    if let Value::Tuple(types) = &type_val {
        for type_item in types {
            if matches_exception_type(state, exc, type_item) {
                return Ok(true);
            }
        }
        return Ok(false);
    }

    Ok(matches_exception_type(state, exc, &type_val))
}

/// Check if an exception matches a single type value.
///
/// Order: universal catch-all → exact name → builtin parent tree →
/// user-class MRO (raised class is subclass of handler class).
fn matches_exception_type(
    state: &InterpreterState,
    exc: &ExceptionValue,
    type_val: &Value,
) -> bool {
    let type_name = match type_val {
        Value::ExceptionType(n) | Value::Class(n) => n.as_str(),
        Value::String(s) => s.as_str(),
        _ => return false,
    };

    if type_name == "Exception" || type_name == "BaseException" {
        return true;
    }
    if exc.type_name == type_name {
        return true;
    }
    if builtin_exception_issubclass(&exc.type_name, type_name) {
        return true;
    }
    matches_user_exception(state, exc, type_name)
}

/// Whether `exc_name` is a subclass of `parent` in the hard-coded
/// builtin tree. Expand as we register more exception constructors.
fn builtin_exception_issubclass(exc_name: &str, parent: &str) -> bool {
    let mut cur = exc_name;
    for _ in 0..16 {
        if cur == parent {
            return true;
        }
        cur = match cur {
            "ZeroDivisionError" | "OverflowError" | "FloatingPointError" => "ArithmeticError",
            "KeyError" | "IndexError" => "LookupError",
            "FileNotFoundError" | "PermissionError" | "TimeoutError" | "IOError" => "OSError",
            "UnicodeDecodeError" | "UnicodeEncodeError" | "UnicodeTranslateError" => "UnicodeError",
            "UnicodeError" => "ValueError",
            "NotImplementedError" | "RecursionError" => "RuntimeError",
            "AssertionError" | "AttributeError" | "NameError" | "TypeError" | "ValueError"
            | "RuntimeError" | "OSError" | "LookupError" | "ArithmeticError" | "StopIteration" => {
                "Exception"
            }
            "Exception" => "BaseException",
            _ => return false,
        };
    }
    false
}

/// `except Handler` matches when the raised type is Handler or a subclass.
/// Walks the raised user class's MRO (and builtin bases on that MRO).
fn matches_user_exception(
    state: &InterpreterState,
    exc: &ExceptionValue,
    handler_name: &str,
) -> bool {
    let Some(raised_class) = state.classes.get(&exc.type_name) else {
        return false;
    };
    for base in &raised_class.mro {
        if base == handler_name {
            return true;
        }
        if builtin_exception_issubclass(base, handler_name) {
            return true;
        }
    }
    false
}

/// Public wrapper for use by the `with` statement machinery in
/// control_flow::build_exit_args. Same body as the file-private
/// `interpreter_error_to_exception`; thin re-export so the
/// abstraction lives in one place.
pub(crate) fn interpreter_error_to_exception_pub(err: &InterpreterError) -> ExceptionValue {
    interpreter_error_to_exception(err)
}

/// Convert an `InterpreterError` to an `ExceptionValue` for except handler matching.
///
/// The `stamp_line` machinery appends ` (at line N)` to error
/// messages so the host-facing pipeline can self-correct. When that
/// error becomes a user-visible Exception (caught by `except ... as e`
/// and inspected via `str(e)`), the suffix is debug archaeology that
/// CPython doesn't include. Strip it here so user code sees clean
/// messages; the host-facing path still gets the stamp from
/// ExceptionValue.stamped_line via Interpreter::execute.
fn interpreter_error_to_exception(err: &InterpreterError) -> ExceptionValue {
    match err {
        InterpreterError::TypeError(msg) => {
            ExceptionValue::new("TypeError", strip_line_marker(msg))
        }
        InterpreterError::ValueError(msg) => {
            ExceptionValue::new("ValueError", strip_line_marker(msg))
        }
        InterpreterError::NameError(msg) => {
            ExceptionValue::new("NameError", strip_line_marker(msg))
        }
        InterpreterError::AttributeError(msg) => {
            ExceptionValue::new("AttributeError", strip_line_marker(msg))
        }
        InterpreterError::AssertionError(msg) => {
            ExceptionValue::new("AssertionError", strip_line_marker(msg))
        }
        InterpreterError::PythonException { type_name, message, .. } => {
            ExceptionValue::new(type_name.clone(), strip_line_marker(message))
        }
        InterpreterError::Runtime(msg) => {
            ExceptionValue::new("RuntimeError", strip_line_marker(msg))
        }
        // Tool failures are catchable as generic Exception (see ToolHandler docs).
        InterpreterError::Tool { tool_name, message } => ExceptionValue::new(
            "Exception",
            format!("ToolError in '{tool_name}': {}", strip_line_marker(message)),
        ),
        _ => ExceptionValue::new("Exception", strip_line_marker(&format!("{err}"))),
    }
}

/// Remove a trailing ` (at line N)` debug suffix from an error
/// message. Returns the original message when no suffix is present.
fn strip_line_marker(msg: &str) -> String {
    if let Some(idx) = msg.rfind(" (at line ") {
        if msg[idx..].ends_with(')') {
            return msg[..idx].to_string();
        }
    }
    msg.to_string()
}

/// Evaluate a raise statement.
pub async fn eval_raise(
    state: &mut InterpreterState,
    node: &ast::StmtRaise,
    tools: &Tools,
) -> EvalResult {
    let Some(exc_expr) = &node.exc else {
        // Bare `raise` re-raises the active exception (the one a
        // surrounding `except` clause is handling). The active stack
        // is pushed by `try_match_handlers` before entering the
        // handler body; outside any handler, CPython raises
        // `RuntimeError: No active exception to re-raise`.
        return state.active_exception_stack.last().cloned().map_or_else(
            || Err(InterpreterError::Runtime("No active exception to re-raise".into()).into()),
            |exc| Err(EvalError::Exception(exc)),
        );
    };

    let exc_val = eval_expr(state, exc_expr, tools).await?;

    // Evaluate cause if present
    let cause = if let Some(ref cause_expr) = node.cause {
        let cause_val = eval_expr(state, cause_expr, tools).await?;
        match cause_val {
            Value::Exception(e) => Some(Box::new(e)),
            _ => None,
        }
    } else {
        None
    };

    // Implicit `__context__` chaining: if we're inside an except
    // handler and the raise has no explicit `from`, CPython attaches
    // the active exception as the new one's `__context__` (rendered as
    // "during handling of the above exception, another exception
    // occurred"). We collapse `__context__` and `__cause__` into the
    // same `cause` field for the user-visible model — they read the
    // same on `.__cause__` / `.__context__`.
    let implicit_context = if cause.is_none() {
        state.active_exception_stack.last().cloned().map(Box::new)
    } else {
        None
    };
    let attached_cause = cause.or(implicit_context);

    match exc_val {
        Value::Exception(mut exc) => {
            exc.cause = attached_cause;
            Err(EvalError::Exception(exc))
        }
        Value::ExceptionType(type_name) => {
            let mut exc = ExceptionValue::new(type_name, String::new());
            if let Some(c) = attached_cause {
                exc = exc.with_cause(*c);
            }
            Err(EvalError::Exception(exc))
        }
        _ => {
            // If the value is a string that names an exception type
            let type_name = format!("{exc_val}");
            if is_exception_type_name(&type_name) {
                let mut exc = ExceptionValue::new(type_name, String::new());
                if let Some(c) = attached_cause {
                    exc = exc.with_cause(*c);
                }
                Err(EvalError::Exception(exc))
            } else {
                Err(InterpreterError::TypeError(format!(
                    "exceptions must derive from BaseException, not '{}'",
                    exc_val.type_name()
                ))
                .into())
            }
        }
    }
}

/// Check if a name is a known exception type.
fn is_exception_type_name(name: &str) -> bool {
    crate::eval::functions::is_exception_type_name(name)
}

/// Evaluate an assert statement.
pub async fn eval_assert(
    state: &mut InterpreterState,
    node: &ast::StmtAssert,
    tools: &Tools,
) -> EvalResult {
    let test = eval_expr(state, &node.test, tools).await?;

    if !crate::eval::op::truthy(state, &test, tools).await? {
        let message = if let Some(ref msg_expr) = node.msg {
            let msg = eval_expr(state, msg_expr, tools).await?;
            format!("{msg}")
        } else {
            String::new()
        };

        return Err(EvalError::Exception(ExceptionValue::new("AssertionError", message)));
    }

    Ok(Value::None)
}
