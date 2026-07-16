// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use rustpython_parser::ast;

use crate::{
    error::{ControlFlow, EvalError, EvalResult, InterpreterError},
    eval::{eval_body, eval_expr, eval_stmt, functions::resolve_proxy, statements::assign_target},
    state::InterpreterState,
    tools::Tools,
    value::Value,
};

/// Evaluate an if statement (including elif chains).
pub async fn eval_if(
    state: &mut InterpreterState,
    node: &ast::StmtIf,
    tools: &Tools,
) -> EvalResult {
    let test = eval_expr(state, &node.test, tools).await?;
    let test = resolve_proxy(&test).await?;

    let cond = match crate::eval::op::try_truthy_sync(&test) {
        Some(b) => b,
        None => crate::eval::op::truthy(state, &test, tools).await?,
    };
    if cond {
        eval_body(state, &node.body, tools).await
    } else {
        // Else / elif chain
        eval_body(state, &node.orelse, tools).await
    }
}

/// Evaluate a for loop.
pub async fn eval_for(
    state: &mut InterpreterState,
    node: &ast::StmtFor,
    tools: &Tools,
) -> EvalResult {
    let iterable = eval_expr(state, &node.iter, tools).await?;
    let iterable = resolve_proxy(&iterable).await?;

    let mut result = Value::None;
    let mut broke = false;

    // Range fast path: walk (start, stop, step) directly without
    // materializing a Vec<Value::Int>. `for i in range(10000)`
    // otherwise allocates and immediately drops 10k integer Values.
    if let Value::Range { start, stop, step } = iterable {
        let pos = step > 0;
        let mut i = start;
        loop {
            let in_range = (pos && i < stop) || (step < 0 && i > stop);
            if !in_range {
                break;
            }
            let body = ForBodyArgs { state, item: Value::Int(i), node, tools };
            let cont = run_for_body(body, &mut result, &mut broke).await?;
            if broke {
                break;
            }
            let _ = cont;
            let Some(next) = i.checked_add(step) else { break };
            i = next;
        }
    } else if matches!(iterable, Value::BuiltinIter { .. } | Value::Generator { .. }) {
        // Step generators / infinite producers lazily rather than
        // draining them up front: `for x in count(): ... break`
        // terminates, and a generator's `try/finally` cleanup runs after
        // the loop body consumes each item (not before the loop starts).
        let empty = indexmap::IndexMap::new();
        loop {
            let item = match crate::eval::functions::dispatch_generator_method(
                state,
                &iterable,
                "__next__",
                &[],
                &empty,
                tools,
            )
            .await
            {
                Ok(v) => v,
                Err(EvalError::Exception(e)) if e.type_name == "StopIteration" => break,
                Err(e) => return Err(e),
            };
            let body = ForBodyArgs { state, item, node, tools };
            let cont = run_for_body(body, &mut result, &mut broke).await?;
            if broke {
                break;
            }
            let _ = cont;
        }
    } else {
        let items = crate::eval::op::iter(state, &iterable, tools).await?;
        for item in items {
            let body = ForBodyArgs { state, item, node, tools };
            let cont = run_for_body(body, &mut result, &mut broke).await?;
            if broke {
                break;
            }
            let _ = cont;
        }
    }

    // Else clause runs only if no break
    if !broke {
        for stmt in &node.orelse {
            result = eval_stmt(state, stmt, tools).await?;
        }
    }

    Ok(result)
}

/// One iteration's worth of for-loop body input. Bundled so the helper
/// stays under the clippy too-many-arguments threshold and so the
/// per-iteration signature reads as a single named context.
struct ForBodyArgs<'a> {
    state: &'a mut InterpreterState,
    item: Value,
    node: &'a ast::StmtFor,
    tools: &'a Tools,
}

/// Run one iteration of a for-loop body. Returns `Ok(true)` to continue,
/// `Ok(false)` for an explicit `break` (caller checks `broke` flag too),
/// and propagates every other error. Extracted so the Range fast path
/// and the materialized-iterator path share the same body semantics.
async fn run_for_body(
    args: ForBodyArgs<'_>,
    result: &mut Value,
    broke: &mut bool,
) -> Result<bool, EvalError> {
    let ForBodyArgs { state, item, node, tools } = args;
    assign_target(state, &node.target, item, tools).await?;

    for stmt in &node.body {
        match eval_stmt(state, stmt, tools).await {
            Ok(val) => {
                *result = val;
            }
            Err(EvalError::Signal(ControlFlow::Continue)) => return Ok(true),
            Err(EvalError::Signal(ControlFlow::Break)) => {
                *broke = true;
                return Ok(false);
            }
            // Return, FinalAnswer, Exception, Interpreter — propagate.
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

/// Evaluate a while loop.
pub async fn eval_while(
    state: &mut InterpreterState,
    node: &ast::StmtWhile,
    tools: &Tools,
) -> EvalResult {
    let max_iters = state.config.max_while_iterations;
    let mut iteration_count = 0u64;
    let mut result = Value::None;
    let mut broke = false;

    loop {
        iteration_count += 1;
        if iteration_count > max_iters {
            return Err(InterpreterError::LimitExceeded(format!(
                "while loop exceeded maximum iterations ({max_iters})"
            ))
            .into());
        }

        let test = eval_expr(state, &node.test, tools).await?;
        let test = resolve_proxy(&test).await?;
        let cond = match crate::eval::op::try_truthy_sync(&test) {
            Some(b) => b,
            None => crate::eval::op::truthy(state, &test, tools).await?,
        };
        if !cond {
            break;
        }

        let mut skip_rest = false;
        for stmt in &node.body {
            match eval_stmt(state, stmt, tools).await {
                Ok(val) => {
                    result = val;
                }
                Err(EvalError::Signal(ControlFlow::Continue)) => {
                    skip_rest = true;
                    break;
                }
                Err(EvalError::Signal(ControlFlow::Break)) => {
                    broke = true;
                    skip_rest = true;
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        if broke {
            break;
        }
        // `skip_rest` is the "continue" signal from the body; the loop
        // naturally re-enters, so no explicit `continue` is needed.
        let _ = skip_rest;
    }

    // Else clause runs only if no break
    if !broke {
        for stmt in &node.orelse {
            result = eval_stmt(state, stmt, tools).await?;
        }
    }

    Ok(result)
}

/// Convert a Value into an iterable Vec of Values.
///
/// # Errors
///
/// Returns an `InterpreterError::TypeError` when `val` is not an iterable
/// type (anything outside List/Tuple/Set/String/Range/Dict).
pub fn iterate_value(val: &Value) -> Result<Vec<Value>, EvalError> {
    // Track A4: route iteration through the TypeObject dispatch layer.
    // Each builtin's iter_slot owns the materialization (see
    // src/types.rs::{sequence_iter, str_iter, bytes_iter, dict_iter,
    // range_iter}); the legacy direct-match version that lived here is
    // gone.
    crate::types::dispatch_iter(val)
}

/// Evaluate a `with` statement.
///
/// CPython 3.12 protocol:
/// 1. For each item in declaration order: evaluate context expression, call `__enter__()`, bind the
///    result to `as <name>` if present.
/// 2. Execute the body. Capture any error/signal that escapes.
/// 3. For each manager in REVERSE order, call `__exit__(exc_type, exc_value, traceback)`. We don't
///    model traceback objects, so we always pass `None` for the third arg.
/// 4. If `__exit__` returns truthy AND we're propagating an exception, suppress the exception.
///    Signals (break/continue/return) propagate unconditionally; they cannot be suppressed.
/// 5. If `__exit__` itself raises, that error REPLACES the current error for the remaining managers
///    and the eventual propagation.
pub async fn eval_with(
    state: &mut InterpreterState,
    node: &ast::StmtWith,
    tools: &Tools,
) -> EvalResult {
    // Phase 1 — enter all context managers in declaration order.
    let mut managers: Vec<Value> = Vec::with_capacity(node.items.len());
    for item in &node.items {
        let cm = eval_expr(state, &item.context_expr, tools).await?;
        let cm = resolve_proxy(&cm).await?;
        let enter_result = call_context_method(state, &cm, "__enter__", &[], tools).await?;
        if let Some(var_expr) = &item.optional_vars {
            assign_target(state, var_expr, enter_result, tools).await?;
        }
        managers.push(cm);
    }

    // Phase 2 — execute body.
    let body_result = eval_body(state, &node.body, tools).await;

    // Phase 3+4 — exit in REVERSE order (with suppression) and propagate.
    exit_context_managers(state, managers, body_result.as_ref().err().cloned(), tools).await?;
    body_result.map_or_else(|_| Ok(Value::None), Ok)
}

/// Call `receiver.method(args)` and, if it returns a coroutine (an `async def`
/// protocol method like `__aenter__`/`__anext__`), drive it to its result.
async fn call_and_drive(
    state: &mut InterpreterState,
    receiver: &Value,
    method: &str,
    args: &[Value],
    tools: &Tools,
) -> EvalResult {
    let result = call_context_method(state, receiver, method, args, tools).await?;
    match result {
        Value::Coroutine(coro) => {
            crate::eval::functions::drive_coroutine(state, &coro, tools).await
        }
        other => Ok(other),
    }
}

/// `async for target in aiter: body`. Drives the async-iterator protocol
/// (`__aiter__` then `__anext__`, awaiting each) sequentially, ending on
/// `StopAsyncIteration`. Reuses the sync for-body machinery — `StmtAsyncFor`
/// carries the same fields as `StmtFor`.
pub async fn eval_async_for(
    state: &mut InterpreterState,
    node: &ast::StmtAsyncFor,
    tools: &Tools,
) -> EvalResult {
    let sync_node = ast::StmtFor {
        target: node.target.clone(),
        iter: node.iter.clone(),
        body: node.body.clone(),
        orelse: node.orelse.clone(),
        type_comment: node.type_comment.clone(),
        range: node.range,
    };
    let iterable = eval_expr(state, &node.iter, tools).await?;
    let iterable = resolve_proxy(&iterable).await?;
    // `__aiter__` returns the async iterator (often `self`); fall back to the
    // object itself if it is already an async iterator without `__aiter__`.
    let aiter = match call_and_drive(state, &iterable, "__aiter__", &[], tools).await {
        Ok(v) => v,
        Err(_) => iterable.clone(),
    };

    // An async generator (`async def` with `yield`) flows through the regular
    // generator machinery, so it is stepped via `__next__`/StopIteration; a user
    // async iterator is stepped via the awaited `__anext__`/StopAsyncIteration.
    let is_generator_source =
        matches!(aiter, Value::Generator { .. } | Value::Lazy { .. } | Value::BuiltinIter { .. });
    let empty = indexmap::IndexMap::new();
    let mut result = Value::None;
    let mut broke = false;
    loop {
        let item = if is_generator_source {
            match crate::eval::functions::dispatch_generator_method(
                state,
                &aiter,
                "__next__",
                &[],
                &empty,
                tools,
            )
            .await
            {
                Ok(v) => v,
                Err(EvalError::Exception(e)) if e.type_name == "StopIteration" => break,
                Err(e) => return Err(e),
            }
        } else {
            match call_and_drive(state, &aiter, "__anext__", &[], tools).await {
                Ok(v) => v,
                Err(EvalError::Exception(e)) if e.type_name == "StopAsyncIteration" => break,
                Err(e) => return Err(e),
            }
        };
        let body = ForBodyArgs { state, item, node: &sync_node, tools };
        let cont = run_for_body(body, &mut result, &mut broke).await?;
        if broke {
            break;
        }
        let _ = cont;
    }
    if !broke {
        for stmt in &sync_node.orelse {
            result = eval_stmt(state, stmt, tools).await?;
        }
    }
    Ok(result)
}

/// `async with cm() as x: body`. Enters/exits via the async context-manager
/// protocol (`__aenter__`/`__aexit__`, awaited), otherwise mirroring `eval_with`.
pub async fn eval_async_with(
    state: &mut InterpreterState,
    node: &ast::StmtAsyncWith,
    tools: &Tools,
) -> EvalResult {
    let mut managers: Vec<Value> = Vec::with_capacity(node.items.len());
    for item in &node.items {
        let cm = eval_expr(state, &item.context_expr, tools).await?;
        let cm = resolve_proxy(&cm).await?;
        let entered = call_and_drive(state, &cm, "__aenter__", &[], tools).await?;
        if let Some(var_expr) = &item.optional_vars {
            assign_target(state, var_expr, entered, tools).await?;
        }
        managers.push(cm);
    }

    let body_result = eval_body(state, &node.body, tools).await;

    // Exit in reverse order, awaiting each `__aexit__` and applying its
    // suppression against the body's error.
    let mut current_error = match &body_result {
        Err(EvalError::Signal(_)) | Ok(_) => None,
        Err(e) => Some(e.clone()),
    };
    let signal_to_propagate = match &body_result {
        Err(e @ EvalError::Signal(_)) => Some(e.clone()),
        _ => None,
    };
    for cm in managers.into_iter().rev() {
        let (is_suppressible, exit_args) = build_exit_args(current_error.as_ref());
        match call_and_drive(state, &cm, "__aexit__", &exit_args, tools).await {
            Ok(v) => {
                if v.is_truthy() && is_suppressible {
                    current_error = None;
                }
            }
            Err(e) => current_error = Some(e),
        }
    }
    if let Some(sig) = signal_to_propagate {
        return Err(sig);
    }
    if let Some(err) = current_error {
        return Err(err);
    }
    body_result.map_or_else(|_| Ok(Value::None), Ok)
}

/// Exit a stack of already-entered context managers in REVERSE order,
/// applying `__exit__`'s suppression rules against `body_err` (the error
/// or signal that escaped the `with` body, if any). Returns `Ok(())` when
/// the block exits cleanly or the body's exception was suppressed; `Err`
/// with the surviving error/signal otherwise. Shared by the eager
/// `eval_with` and the generator `with`-suspend stepper so both apply
/// identical exit semantics.
pub(crate) async fn exit_context_managers(
    state: &mut InterpreterState,
    managers: Vec<Value>,
    body_err: Option<EvalError>,
    tools: &Tools,
) -> Result<(), EvalError> {
    // Signals (break/continue/return) propagate unconditionally — `__exit__`
    // still runs, but its truthy return cannot suppress them.
    let signal_to_propagate = match &body_err {
        Some(EvalError::Signal(_)) => body_err.clone(),
        _ => None,
    };
    let mut current_error = match &body_err {
        Some(EvalError::Signal(_)) | None => None,
        _ => body_err,
    };
    for cm in managers.into_iter().rev() {
        let (is_suppressible, exit_args) = build_exit_args(current_error.as_ref());
        match call_context_method(state, &cm, "__exit__", &exit_args, tools).await {
            Ok(v) => {
                if v.is_truthy() && is_suppressible {
                    current_error = None;
                }
            }
            Err(exit_err) => {
                current_error = Some(exit_err);
            }
        }
    }
    if let Some(sig) = signal_to_propagate {
        return Err(sig);
    }
    if let Some(err) = current_error {
        return Err(err);
    }
    Ok(())
}

/// Construct the `(exc_type, exc_value, traceback)` triple for an
/// `__exit__` call, plus a flag indicating whether the error is the
/// kind that `__exit__` can suppress. Returns `(false, [None, None,
/// None])` for the no-error case and for signal control flow (which
/// cannot be suppressed).
fn build_exit_args(current_error: Option<&EvalError>) -> (bool, Vec<Value>) {
    let nones = vec![Value::None, Value::None, Value::None];
    let Some(err) = current_error else {
        return (false, nones);
    };
    match err {
        EvalError::Signal(_) => (false, nones),
        EvalError::Exception(exc) => (
            true,
            vec![
                Value::ExceptionType(exc.type_name.clone()),
                Value::Exception(Box::new(exc.clone())),
                Value::None,
            ],
        ),
        EvalError::Interpreter(ie) => {
            // Convert the interpreter-level error to an exception triple
            // so user __exit__ can introspect it the same way CPython
            // would (TypeError, NameError, etc. ARE exceptions in
            // CPython; our Interpreter variant is an implementation
            // detail of how we surface them).
            let exc = crate::eval::exceptions::interpreter_error_to_exception_pub(ie);
            (
                true,
                vec![
                    Value::ExceptionType(exc.type_name.clone()),
                    Value::Exception(Box::new(exc)),
                    Value::None,
                ],
            )
        }
    }
}

/// Call a dunder method (`__enter__` / `__exit__`) on a value. Routes
/// through `instance_method_call` for user-class instances. Builtin
/// types don't have native context-manager support in this
/// implementation; if support is needed for `with open(...)`-style
/// patterns, register a per-type dispatch here.
pub(crate) async fn call_context_method(
    state: &mut InterpreterState,
    receiver: &Value,
    method: &str,
    args: &[Value],
    tools: &Tools,
) -> EvalResult {
    // `redirect_stdout` needs `&mut state` to push/pop the redirect stack, so
    // it is handled here rather than in the sync `try_contextlib_method`.
    if let Value::Instance(inst) = receiver {
        if inst.class_name == crate::eval::modules::contextlib_mod::REDIRECT_STDOUT_CLASS {
            let target = inst.fields.lock().get("target").cloned();
            match (method, target) {
                ("__enter__", Some(Value::StringIO(io))) => {
                    state.stdout_redirects.push(io.clone());
                    return Ok(Value::StringIO(io));
                }
                ("__exit__", _) => {
                    state.stdout_redirects.pop();
                    return Ok(Value::Bool(false));
                }
                _ => {}
            }
        }
    }
    // A StringIO is its own context manager (`with io.StringIO() as f:`).
    if let Value::StringIO(io) = receiver {
        return match method {
            "__enter__" => Ok(Value::StringIO(io.clone())),
            "__exit__" => Ok(Value::Bool(false)),
            _ => Err(InterpreterError::AttributeError(format!(
                "'_io.StringIO' object has no attribute '{method}'"
            ))
            .into()),
        };
    }
    // contextlib.nullcontext / suppress — marker instances without Python methods.
    if let Some(result) =
        crate::eval::modules::contextlib_mod::try_contextlib_method(state, receiver, method, args)
    {
        return result;
    }
    // `@contextmanager` instances step a suspended generator (async).
    if let Some(result) =
        crate::eval::modules::contextlib_mod::try_gencm_method(state, receiver, method, args, tools)
            .await
    {
        return result;
    }
    // `ExitStack` __enter__/__exit__ (unwinds its registered cleanups).
    if let Some(result) = crate::eval::modules::contextlib_mod::try_exitstack_method(
        state, receiver, method, args, tools,
    )
    .await
    {
        return result;
    }
    if let Some(result) =
        crate::eval::modules::decimal::try_localcontext_method(state, receiver, method, args)
    {
        return result;
    }
    if !matches!(receiver, Value::Instance(_)) {
        return Err(InterpreterError::AttributeError(format!(
            "'{}' object has no attribute '{method}'",
            receiver.type_name()
        ))
        .into());
    }
    let kwargs = indexmap::IndexMap::new();
    let call = crate::eval::functions::CallArgs { positional: args, keyword: &kwargs };
    let (returned, _self) =
        crate::eval::classes::instance_method_call(state, receiver.clone(), method, call, tools)
            .await?;
    Ok(returned)
}
