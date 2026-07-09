// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use rustpython_parser::ast::{Expr, Ranged, Stmt};

use crate::{
    error::{ControlFlow, EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    tools::Tools,
    value::Value,
};

/// 1-based line number that byte `offset` falls on inside `source`.
/// Returns 1 if `offset` is past the end (caller passed a degenerate
/// or default range), so a stamped message still names *some* line
/// rather than swallowing the diagnostic entirely.
pub(crate) fn line_of(source: &str, offset: usize) -> usize {
    if offset > source.len() {
        return 1;
    }
    source[..offset].bytes().filter(|b| *b == b'\n').count() + 1
}

/// Stamp ` (at line N)` onto an error's user-visible message when it
/// doesn't already carry a line marker. Agent loops persist this string
/// as `errorMessage`; without the line they can't self-correct beyond
/// the simplest one-statement scripts.
///
/// Control-flow signals pass through unchanged — they're never surfaced
/// to the user. `Syntax` already carries line/col from the parser.
/// `RecursionLimitExceeded` and `StateFormatSuperseded` describe state
/// the source line wouldn't clarify.
pub(crate) fn stamp_line(err: EvalError, line: usize) -> EvalError {
    let suffix = format!(" (at line {line})");
    let already_stamped = |s: &str| s.contains("at line ");
    let line_u32 = u32::try_from(line).unwrap_or(u32::MAX);
    match err {
        EvalError::Signal(_) => err,
        EvalError::Interpreter(inner) => EvalError::Interpreter(match inner {
            InterpreterError::Syntax(_)
            | InterpreterError::RecursionLimitExceeded { .. }
            | InterpreterError::StateFormatSuperseded { .. } => inner,
            InterpreterError::Security(m) if !already_stamped(&m) => {
                InterpreterError::Security(format!("{m}{suffix}"))
            }
            InterpreterError::Runtime(m) if !already_stamped(&m) => {
                InterpreterError::Runtime(format!("{m}{suffix}"))
            }
            InterpreterError::LimitExceeded(m) if !already_stamped(&m) => {
                InterpreterError::LimitExceeded(format!("{m}{suffix}"))
            }
            InterpreterError::NameError(m) if !already_stamped(&m) => {
                InterpreterError::NameError(format!("{m}{suffix}"))
            }
            InterpreterError::TypeError(m) if !already_stamped(&m) => {
                InterpreterError::TypeError(format!("{m}{suffix}"))
            }
            InterpreterError::ValueError(m) if !already_stamped(&m) => {
                InterpreterError::ValueError(format!("{m}{suffix}"))
            }
            InterpreterError::AttributeError(m) if !already_stamped(&m) => {
                InterpreterError::AttributeError(format!("{m}{suffix}"))
            }
            InterpreterError::AssertionError(m) if !already_stamped(&m) => {
                InterpreterError::AssertionError(format!("{m}{suffix}"))
            }
            InterpreterError::Tool { tool_name, message } if !already_stamped(&message) => {
                InterpreterError::Tool { tool_name, message: format!("{message}{suffix}") }
            }
            other => other,
        }),
        // Exception variants: stamp via a SIDE field, not by mutating
        // message. Display() for Value::Exception renders just the
        // message so `print(e)` / `str(e)` / `f'{e}'` inside the user
        // script stay clean. The Interpreter::execute boundary
        // appends `(at line N)` from stamped_line to the final
        // host-facing error message — that's where the agent loop
        // wants the suffix.
        EvalError::Exception(exc) if exc.stamped_line.is_none() => {
            let mut rebuilt = exc;
            rebuilt.stamped_line = Some(line_u32);
            EvalError::Exception(rebuilt)
        }
        EvalError::Exception(exc) => EvalError::Exception(exc),
    }
}

pub mod classes;
pub mod comprehensions;
pub mod control_flow;
pub mod delete;
pub mod exceptions;
pub mod functions;
pub mod literals;
pub mod match_stmt;
pub mod modules;
pub mod names;
pub mod op;
pub mod operations;
pub mod place;
pub mod render;
pub mod statements;
pub mod strings;

/// Evaluate a single statement.
///
/// Uses `Box::pin` internally to handle recursive async calls.
pub fn eval_stmt<'a>(
    state: &'a mut InterpreterState,
    stmt: &'a Stmt,
    tools: &'a Tools,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = EvalResult> + Send + 'a>> {
    Box::pin(async move {
        state.increment_ops().map_err(EvalError::Interpreter)?;

        // Compute the statement's start line so a downstream error can be
        // stamped with `(at line N)`. Done before dispatch so the innermost
        // statement (deepest in the recursion) stamps first; outer
        // statements observe the marker and skip — preserving the most
        // specific line for the agent loop.
        //
        // When we're executing inside a function/lambda body, the byte
        // offsets in the body's AST nodes point into the source that
        // *defined* the body — not into the current execute()'s source.
        // call_user_function / call_lambda push their body's source on
        // `body_source_stack` before recursing; consult the top here so
        // a persisted function called in a later execute() stamps with
        // the correct line.
        let stmt_line = {
            let active_source = state
                .body_source_stack
                .last()
                .map_or(state.current_source.as_str(), String::as_str);
            line_of(active_source, stmt.range().start().to_usize())
        };

        let result = match stmt {
            // Bare-expression statements (`docstring`, top-level `42`)
            // whose value is a Constant are no-ops — Python evaluates
            // them but the result is discarded. Skip the eval_expr
            // recursion entirely; the line-stamp + op-count above
            // already covered the per-statement overhead.
            Stmt::Expr(node) if matches!(node.value.as_ref(), Expr::Constant(_)) => Ok(Value::None),
            Stmt::Expr(node) => eval_expr(state, &node.value, tools).await,
            Stmt::Assign(node) => statements::eval_assign(state, node, tools).await,
            Stmt::AugAssign(node) => statements::eval_aug_assign(state, node, tools).await,
            Stmt::AnnAssign(node) => statements::eval_ann_assign(state, node, tools).await,
            Stmt::If(node) => control_flow::eval_if(state, node, tools).await,
            Stmt::For(node) => control_flow::eval_for(state, node, tools).await,
            Stmt::While(node) => control_flow::eval_while(state, node, tools).await,
            Stmt::Break(_) => Err(EvalError::Signal(ControlFlow::Break)),
            Stmt::Continue(_) => Err(EvalError::Signal(ControlFlow::Continue)),
            Stmt::Return(node) => {
                // Inline `?` would short-circuit past `result.map_err`
                // below and ship the underlying error un-stamped — the
                // very bug B1/B2 are meant to fix. Match explicitly so
                // the error bubbles through the post-match stamp.
                let val_result = if let Some(ref v) = node.value {
                    eval_expr(state, v, tools).await
                } else {
                    Ok(Value::None)
                };
                match val_result {
                    Ok(val) => Err(EvalError::Signal(ControlFlow::Return(Box::new(val)))),
                    Err(e) => Err(e),
                }
            }
            Stmt::FunctionDef(node) => functions::eval_function_def(state, node, tools).await,
            Stmt::Try(node) => exceptions::eval_try(state, node, tools).await,
            Stmt::Raise(node) => exceptions::eval_raise(state, node, tools).await,
            Stmt::Assert(node) => exceptions::eval_assert(state, node, tools).await,
            Stmt::Delete(node) => delete::eval_delete(state, node, tools).await,
            Stmt::Match(node) => match_stmt::eval_match(state, node, tools).await,
            Stmt::With(node) => control_flow::eval_with(state, node, tools).await,
            Stmt::Import(node) => modules::eval_import(state, node),
            Stmt::ImportFrom(node) => modules::eval_import_from(state, node),
            Stmt::ClassDef(node) => classes::eval_class_def(state, node, tools).await,
            // Pass, Global, and Nonlocal are all interpreter-level no-ops —
            // Python semantics don't require scope annotations for our
            // runtime.
            Stmt::Pass(_) | Stmt::Global(_) | Stmt::Nonlocal(_) => Ok(Value::None),
            _ => Err(InterpreterError::Runtime(format!(
                "unsupported statement: {:?}",
                std::mem::discriminant(stmt)
            ))
            .into()),
        };
        result.map_err(|e| stamp_line(e, stmt_line))
    })
}

/// Try to evaluate an expression synchronously, without allocating a
/// `Box::pin`'d future. Returns `Some(result)` for leaf shapes whose
/// evaluation needs no async dispatch (Constant literals, plain Name
/// reads); returns `None` for every other shape, signalling the caller
/// to fall back to the async [`eval_expr`] path.
///
/// `Expr::Constant` and `Expr::Name` together cover the bulk of inner-
/// loop expression evaluations on realistic LLM-emitted snippets — every
/// `i + 1`, `d[5]`, `r["category"]` is a small tree whose leaves are one
/// or both of those. Skipping the `Box::pin` allocation per leaf saves
/// ~64 B of heap traffic per call and removes a future-state-machine
/// poll cycle that the optimiser otherwise can't elide.
///
/// Op-count increment still happens here — the budget cap doesn't get
/// to look the other way because the path is sync.
#[inline]
pub fn try_eval_expr_sync(
    state: &mut InterpreterState,
    expr: &Expr,
    tools: &Tools,
) -> Option<EvalResult> {
    match expr {
        Expr::Constant(node) => {
            if let Err(e) = state.increment_ops() {
                return Some(Err(EvalError::Interpreter(e)));
            }
            Some(Ok(literals::eval_constant(&node.value)))
        }
        Expr::Name(node) => {
            if let Err(e) = state.increment_ops() {
                return Some(Err(EvalError::Interpreter(e)));
            }
            Some(names::eval_name(state, node, tools))
        }
        _ => None,
    }
}

/// Evaluate a single expression.
///
/// Uses `Box::pin` internally to handle recursive async calls. Hot
/// callers should try [`try_eval_expr_sync`] first to skip the boxed
/// future on Constant / Name leaves.
pub fn eval_expr<'a>(
    state: &'a mut InterpreterState,
    expr: &'a Expr,
    tools: &'a Tools,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = EvalResult> + Send + 'a>> {
    Box::pin(async move {
        state.increment_ops().map_err(EvalError::Interpreter)?;

        match expr {
            Expr::Constant(node) => Ok(literals::eval_constant(&node.value)),
            Expr::List(node) => literals::eval_list(state, node, tools).await,
            Expr::Tuple(node) => literals::eval_tuple(state, node, tools).await,
            Expr::Dict(node) => literals::eval_dict(state, node, tools).await,
            Expr::Set(node) => literals::eval_set(state, node, tools).await,
            Expr::Name(node) => names::eval_name(state, node, tools),
            Expr::Attribute(node) => names::eval_attribute(state, node, tools).await,
            Expr::Subscript(node) => names::eval_subscript(state, node, tools).await,
            Expr::BinOp(node) => operations::eval_binop(state, node, tools).await,
            Expr::UnaryOp(node) => operations::eval_unaryop(state, node, tools).await,
            Expr::Compare(node) => operations::eval_compare(state, node, tools).await,
            Expr::BoolOp(node) => operations::eval_boolop(state, node, tools).await,
            Expr::IfExp(node) => operations::eval_ifexp(state, node, tools).await,
            Expr::Call(node) => functions::eval_call(state, node, tools).await,
            Expr::Lambda(node) => functions::eval_lambda_def(state, node, tools).await,
            Expr::JoinedStr(node) => strings::eval_joined_str(state, node, tools).await,
            Expr::FormattedValue(node) => strings::eval_formatted_value(state, node, tools).await,
            Expr::ListComp(node) => comprehensions::eval_list_comp(state, node, tools).await,
            Expr::DictComp(node) => comprehensions::eval_dict_comp(state, node, tools).await,
            Expr::SetComp(node) => comprehensions::eval_set_comp(state, node, tools).await,
            Expr::GeneratorExp(node) => {
                comprehensions::eval_generator_exp(state, node, tools).await
            }
            Expr::NamedExpr(node) => names::eval_named_expr(state, node, tools).await,
            Expr::Starred(node) => eval_expr(state, &node.value, tools).await,
            Expr::Slice(node) => names::eval_slice(state, node, tools).await,
            // Track C: yield inside a generator function body pushes
            // the yielded value onto the current yield-buffer frame.
            // The yield expression evaluates to None (we don't support
            // gen.send() yet — that requires a real coroutine).
            Expr::Yield(node) => {
                let value = if let Some(ref v) = node.value {
                    eval_expr(state, v, tools).await?
                } else {
                    crate::value::Value::None
                };
                let Some(buffer) = state.yield_stack.last_mut() else {
                    return Err(crate::error::InterpreterError::Runtime(
                        "'yield' outside function: yield can only appear inside a generator function".into(),
                    )
                    .into());
                };
                buffer.push(value);
                Ok(crate::value::Value::None)
            }
            // `yield from <iterable>` — delegate every value out
            // through the current buffer.
            Expr::YieldFrom(node) => {
                let source = eval_expr(state, &node.value, tools).await?;
                let items = crate::eval::op::iter(state, &source, tools).await?;
                let Some(buffer) = state.yield_stack.last_mut() else {
                    return Err(crate::error::InterpreterError::Runtime(
                        "'yield from' outside function: yield from can only appear inside a generator function".into(),
                    )
                    .into());
                };
                buffer.extend(items);
                Ok(crate::value::Value::None)
            }
            // `await` requires the full async / coroutine machinery
            // (PEP 492), which is out of scope per CONFORMANCE.md.
            Expr::Await(_) => Err(InterpreterError::Runtime(
                "'await' is not supported (see CONFORMANCE.md#unsupported-language-features)"
                    .into(),
            )
            .into()),
        }
    })
}

/// Evaluate a list of statements, returning the last value.
pub async fn eval_body(state: &mut InterpreterState, body: &[Stmt], tools: &Tools) -> EvalResult {
    let mut result = Value::None;
    for stmt in body {
        result = eval_stmt(state, stmt, tools).await?;
    }
    Ok(result)
}
