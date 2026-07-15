// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Generator functions with true yield/resume frames.
//!
//! Calling a generator `def` returns [`Value::Generator`] without running the
//! body. Each `next`/`send` restores frame locals, runs until the next
//! `yield` (via [`ControlFlow::Yield`]), saves locals, and returns the
//! yielded value. Memory stays O(frame), not O(all yields).
//!
//! Eager [`Value::Lazy`] buffers remain for generator *expressions*
//! (`(x for x in ...)`) and any nested yield_stack collection path.

use std::sync::Arc;

use indexmap::IndexMap;
use rustpython_parser::ast::Stmt;

use crate::{
    error::{ControlFlow, EvalError, EvalResult, InterpreterError},
    eval::{eval_stmt, statements::assign_target},
    state::{GeneratorForState, GeneratorFrame, InterpreterState},
    value::{ExceptionValue, FunctionDef, Value},
};

/// True when `method` is a generator-iterator protocol name.
#[must_use]
pub(crate) fn is_generator_method(method: &str) -> bool {
    matches!(method, "send" | "throw" | "close" | "__next__")
}

/// `while` loops need extra resume state not yet modelled — callers fall
/// back to the eager Lazy buffer for those generator bodies.
#[must_use]
pub(crate) fn body_has_while(stmts: &[Stmt]) -> bool {
    use rustpython_parser::ast::ExceptHandler;
    stmts.iter().any(|s| match s {
        Stmt::While(_) => true,
        Stmt::For(f) => body_has_while(&f.body) || body_has_while(&f.orelse),
        Stmt::If(i) => body_has_while(&i.body) || body_has_while(&i.orelse),
        Stmt::With(w) => body_has_while(&w.body),
        Stmt::Try(t) => {
            body_has_while(&t.body)
                || t.handlers.iter().any(|h| {
                    let ExceptHandler::ExceptHandler(eh) = h;
                    body_has_while(&eh.body)
                })
                || body_has_while(&t.orelse)
                || body_has_while(&t.finalbody)
        }
        Stmt::TryStar(t) => {
            body_has_while(&t.body)
                || t.handlers.iter().any(|h| {
                    let ExceptHandler::ExceptHandler(eh) = h;
                    body_has_while(&eh.body)
                })
                || body_has_while(&t.orelse)
                || body_has_while(&t.finalbody)
        }
        _ => false,
    })
}

/// Whether a generator body can use the suspend path. True when it has no
/// `while` loop, or when every `while` is a TOP-LEVEL statement of the body
/// whose `yield`s are all *direct* statements of the while body — the only
/// shape the top-level while-resume machinery handles exactly. A `while` nested
/// inside another block, one with an `else`, or one whose yields sit inside a
/// nested `if`/`for`/`try` (which `body_index` resumption would re-run) forces
/// the eager Lazy buffer, preserving the pre-existing behaviour.
#[must_use]
pub(crate) fn generator_suspendable(stmts: &[Stmt]) -> bool {
    for stmt in stmts {
        match stmt {
            Stmt::While(w) => {
                if !w.orelse.is_empty()
                    || body_has_while(&w.body)
                    || !while_yields_are_direct(&w.body)
                {
                    return false;
                }
            }
            // A `while` nested inside any other statement is not suspendable.
            other if body_has_while(std::slice::from_ref(other)) => return false,
            _ => {}
        }
    }
    true
}

/// Whether every `yield` in a while body is a direct statement of that body
/// (so a `body_index` cursor resumes exactly at it). A yield buried in a nested
/// compound statement would be re-executed from the top on resume.
fn while_yields_are_direct(body: &[Stmt]) -> bool {
    body.iter().all(|stmt| {
        match stmt {
            // Compound statements that could hide a yield they can't resume at.
            Stmt::If(_)
            | Stmt::For(_)
            | Stmt::While(_)
            | Stmt::With(_)
            | Stmt::Try(_)
            | Stmt::TryStar(_)
            | Stmt::Match(_) => {
                !super::definitions::contains_yield_stmts(std::slice::from_ref(stmt))
            }
            // Simple statements (including a bare `yield` expr statement, or an
            // assignment whose RHS yields) resume cleanly.
            _ => true,
        }
    })
}

/// Create a suspended generator from a just-bound function frame.
pub(crate) fn create_generator(
    state: &mut InterpreterState,
    func_def: &FunctionDef,
    body: Arc<Vec<Stmt>>,
    locals: rustc_hash::FxHashMap<String, Value>,
    touched: Vec<String>,
) -> Value {
    let id = state.next_cursor_id;
    state.next_cursor_id = state.next_cursor_id.wrapping_add(1);
    state.generators.insert(
        id,
        GeneratorFrame {
            func_name: func_def.name.clone(),
            source: func_def.source.clone(),
            body,
            touched,
            locals,
            started: false,
            finished: false,
            closed: false,
            send_value: Value::None,
            resume_at_yield: false,
            pending_throw: None,
            stmt_index: 0,
            for_stack: Vec::new(),
            while_resume: None,
            try_stack: Vec::new(),
            yield_from_return: None,
        },
    );
    Value::Generator { id }
}

/// Dispatch a generator method on `Value::Generator` or legacy `Value::Lazy`.
pub(crate) async fn dispatch_generator_method(
    state: &mut InterpreterState,
    receiver: &Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &crate::tools::Tools,
) -> EvalResult {
    if let Some((name, _)) = kwargs.first() {
        return Err(InterpreterError::TypeError(format!(
            "{method}() got an unexpected keyword argument '{name}'"
        ))
        .into());
    }

    if let Value::Generator { id } = receiver {
        return dispatch_suspended(state, *id, method, args, tools).await;
    }

    if let Value::BuiltinIter { id, .. } = receiver {
        return dispatch_builtin_iter(state, receiver, *id, method, args);
    }

    // Legacy eager Lazy buffer path.
    dispatch_lazy(state, receiver, method, args)
}

async fn dispatch_suspended(
    state: &mut InterpreterState,
    id: u64,
    method: &str,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    match method {
        "__next__" => {
            if !args.is_empty() {
                return Err(
                    InterpreterError::TypeError("__next__() takes no arguments".into()).into()
                );
            }
            step_generator(state, id, Value::None, tools).await
        }
        "send" => {
            let value = args.first().cloned().unwrap_or(Value::None);
            let frame = state
                .generators
                .get(&id)
                .ok_or_else(|| InterpreterError::Runtime("generator frame missing".into()))?;
            if !frame.started && !matches!(value, Value::None) {
                return Err(InterpreterError::TypeError(
                    "can't send non-None value to a just-started generator".into(),
                )
                .into());
            }
            step_generator(state, id, value, tools).await
        }
        "throw" => throw_into_generator(state, id, args, tools).await,
        "close" => {
            // A live suspended generator is closed by throwing
            // `GeneratorExit` at its yield, so any pending `finally`
            // (and `with`) cleanup runs. A not-started / finished
            // generator has nothing to run.
            let live = state
                .generators
                .get(&id)
                .is_some_and(|f| f.started && !f.finished && !f.closed && f.resume_at_yield);
            if live {
                let gexit = Value::Exception(Box::new(crate::value::ExceptionValue::new(
                    "GeneratorExit",
                    "",
                )));
                let result =
                    throw_into_generator(state, id, std::slice::from_ref(&gexit), tools).await;
                mark_generator_closed(state, id);
                return match result {
                    // Exited cleanly (GeneratorExit / StopIteration propagated).
                    Err(EvalError::Exception(e))
                        if e.type_name == "GeneratorExit" || e.type_name == "StopIteration" =>
                    {
                        Ok(Value::None)
                    }
                    // Yielded again → ignored GeneratorExit (CPython: RuntimeError).
                    Ok(_) => {
                        Err(InterpreterError::Runtime("generator ignored GeneratorExit".into())
                            .into())
                    }
                    Err(e) => Err(e),
                };
            }
            mark_generator_closed(state, id);
            Ok(Value::None)
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'generator' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Finalise every still-suspended generator at the end of a run — the
/// analogue of CPython closing outstanding generators at interpreter
/// shutdown, so a `try/finally` or `with` left open in a generator runs
/// its cleanup. Each is closed via `GeneratorExit` (running its
/// `finally`); errors from a misbehaving generator are swallowed so one
/// bad finaliser can't fail the run. Finalises in creation order (the
/// `id` is monotonic), matching CPython's shutdown finalisation here.
pub(crate) async fn finalize_generators(state: &mut InterpreterState, tools: &crate::tools::Tools) {
    let mut ids: Vec<u64> = state
        .generators
        .iter()
        .filter(|(_, f)| f.started && !f.finished && !f.closed)
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    let empty = IndexMap::new();
    for id in ids {
        let live = state.generators.get(&id).is_some_and(|f| f.started && !f.finished && !f.closed);
        if !live {
            continue;
        }
        let _ =
            dispatch_generator_method(state, &Value::Generator { id }, "close", &[], &empty, tools)
                .await;
    }
}

/// Mark a generator as finished/closed and drop its loop-resume state.
fn mark_generator_closed(state: &mut InterpreterState, id: u64) {
    if let Some(frame) = state.generators.get_mut(&id) {
        frame.finished = true;
        frame.closed = true;
        frame.for_stack.clear();
        frame.try_stack.clear();
    }
}

/// `generator.throw(exc)` — inject `exc` at the generator's suspended `yield`
/// and resume, so the generator's own `try/except` can catch it (and possibly
/// yield again, which becomes throw's return value). A not-yet-started or
/// already-finished generator has no live suspension point, so the exception
/// propagates straight to the caller.
async fn throw_into_generator(
    state: &mut InterpreterState,
    id: u64,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let exc = match args.first() {
        Some(Value::Exception(e)) => (**e).clone(),
        Some(Value::ExceptionType(n)) => {
            let msg = args.get(1).map(|v| format!("{v}")).unwrap_or_default();
            ExceptionValue::new(n.clone(), msg)
        }
        Some(other) => {
            return Err(InterpreterError::TypeError(format!(
                "exceptions must derive from BaseException, not '{}'",
                other.type_name()
            ))
            .into());
        }
        None => {
            return Err(
                InterpreterError::TypeError("throw() takes at least 1 argument".into()).into()
            );
        }
    };
    let live = state
        .generators
        .get(&id)
        .is_some_and(|f| f.started && !f.finished && !f.closed && f.resume_at_yield);
    if !live {
        // No suspended yield to catch it: finish the generator and raise out.
        if let Some(frame) = state.generators.get_mut(&id) {
            frame.finished = true;
            frame.closed = true;
        }
        return Err(EvalError::Exception(exc));
    }
    if let Some(frame) = state.generators.get_mut(&id) {
        frame.pending_throw = Some(Box::new(exc));
    }
    step_generator(state, id, Value::None, tools).await
}

async fn step_generator(
    state: &mut InterpreterState,
    id: u64,
    send_value: Value,
    tools: &crate::tools::Tools,
) -> EvalResult {
    // Snapshot / mutate frame under a short borrow, then run the body.
    let (locals_snapshot, source, touched, body, mut stmt_index, early) = {
        let frame = state
            .generators
            .get_mut(&id)
            .ok_or_else(|| InterpreterError::Runtime("generator frame missing".into()))?;
        if frame.closed || frame.finished {
            return Err(EvalError::Exception(ExceptionValue::new("StopIteration", String::new())));
        }

        // Drain yield-from remainder without re-entering body when possible.
        if !frame.for_stack.is_empty() && frame.started {
            let top = frame
                .for_stack
                .last_mut()
                .ok_or_else(|| InterpreterError::Runtime("for_stack empty".into()))?;
            if top.target.is_empty() {
                if top.pos < top.items.len() {
                    let v = top.items[top.pos].clone();
                    top.pos += 1;
                    if top.pos >= top.items.len() {
                        frame.for_stack.pop();
                    }
                    return Ok(v);
                }
                frame.for_stack.pop();
            }
        }

        frame.started = true;
        frame.send_value = send_value;
        // `resume_at_yield` is set when we suspend on Yield; do not clear
        // it here. send() only updates the value delivered to that yield.

        let locals_snapshot: Vec<(String, Value)> =
            frame.locals.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        (
            locals_snapshot,
            frame.source.clone(),
            frame.touched.clone(),
            frame.body.clone(),
            frame.stmt_index,
            false,
        )
    };
    let _ = early;

    for (k, v) in locals_snapshot {
        let _ = state.set_variable(&k, v);
    }
    state.body_source_stack.push(source);
    state.active_generator_stack.push(id);

    let result = run_generator_body(state, &body, &mut stmt_index, tools).await;

    let _ = state.active_generator_stack.pop();
    state.body_source_stack.pop();

    if let Some(frame) = state.generators.get_mut(&id) {
        frame.stmt_index = stmt_index;
        for name in &touched {
            if let Some(v) = state.variables.get(name) {
                frame.locals.insert(name.clone(), v.clone());
            }
        }
    }

    match result {
        Ok(()) => {
            if let Some(frame) = state.generators.get_mut(&id) {
                frame.finished = true;
            }
            Err(EvalError::Exception(ExceptionValue::new("StopIteration", String::new())))
        }
        Err(EvalError::Signal(ControlFlow::Yield(v))) => Ok(*v),
        Err(EvalError::Signal(ControlFlow::Return(v))) => {
            if let Some(frame) = state.generators.get_mut(&id) {
                frame.finished = true;
            }
            // Carry the generator's return value in StopIteration so a
            // delegating `yield from` can recover it (CPython's `e.value`).
            Err(EvalError::Exception(stop_iteration_with_value(*v)))
        }
        Err(e) => {
            if let Some(frame) = state.generators.get_mut(&id) {
                frame.finished = true;
            }
            Err(e)
        }
    }
}

/// Build the `StopIteration` that ends a generator, carrying its `return`
/// value as `args[0]` (CPython's `e.value`). A `None` return leaves `args`
/// empty so `str(exc)`/`.value` behave like a bare `return`.
pub(crate) fn stop_iteration_with_value(value: Value) -> ExceptionValue {
    let exc = ExceptionValue::new("StopIteration", String::new());
    if matches!(value, Value::None) { exc } else { exc.with_args(vec![value]) }
}

async fn run_generator_body(
    state: &mut InterpreterState,
    body: &[Stmt],
    stmt_index: &mut usize,
    tools: &crate::tools::Tools,
) -> Result<(), EvalError> {
    // Resume an open for-loop before advancing top-level statements.
    if let Some(&id) = state.active_generator_stack.last() {
        if let Some(frame) = state.generators.get(&id) {
            if let Some(fs) = frame.for_stack.last() {
                if !fs.target.is_empty() {
                    return resume_for(state, tools).await;
                }
            }
        }
    }

    while *stmt_index < body.len() {
        let stmt = &body[*stmt_index];
        // Special-case top-level for with simple name target for suspend.
        if let Stmt::For(for_node) = stmt {
            if let rustpython_parser::ast::Expr::Name(t) = for_node.target.as_ref() {
                let target = t.id.as_str().to_string();
                match run_for_suspendable(state, for_node, &target, tools).await {
                    Ok(()) => {
                        *stmt_index += 1;
                        continue;
                    }
                    Err(EvalError::Signal(ControlFlow::Yield(v))) => {
                        return Err(EvalError::Signal(ControlFlow::Yield(v)));
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        // Top-level while loop with a suspendable body (see
        // `generator_suspendable`, which gates the whole generator onto the
        // suspend path only when this holds).
        if let Stmt::While(while_node) = stmt {
            match run_while_suspendable(state, while_node, tools).await {
                Ok(()) => {
                    *stmt_index += 1;
                    continue;
                }
                Err(EvalError::Signal(ControlFlow::Yield(v))) => {
                    return Err(EvalError::Signal(ControlFlow::Yield(v)));
                }
                Err(e) => return Err(e),
            }
        }
        // Top-level try: step the body statement-by-statement so multiple
        // yields each resume at their own statement and `finally` runs on
        // the real exit — not the eager `eval_try` path, which would
        // re-run the whole body and run finally on suspend.
        if let Stmt::Try(try_node) = stmt {
            match run_try_suspendable(state, try_node, tools).await {
                Ok(()) => {
                    *stmt_index += 1;
                    continue;
                }
                Err(EvalError::Signal(ControlFlow::Yield(v))) => {
                    return Err(EvalError::Signal(ControlFlow::Yield(v)));
                }
                Err(e) => return Err(e),
            }
        }

        match eval_stmt(state, stmt, tools).await {
            Ok(_) => {
                *stmt_index += 1;
                // After completing a resumed yield statement, clear resume flag path.
                if let Some(&id) = state.active_generator_stack.last() {
                    if let Some(frame) = state.generators.get_mut(&id) {
                        frame.resume_at_yield = false;
                    }
                }
            }
            Err(EvalError::Signal(ControlFlow::Yield(v))) => {
                // Stay on this statement so resume re-enters the yield expr.
                if let Some(&id) = state.active_generator_stack.last() {
                    if let Some(frame) = state.generators.get_mut(&id) {
                        frame.resume_at_yield = true;
                    }
                }
                return Err(EvalError::Signal(ControlFlow::Yield(v)));
            }
            // Propagate the return signal so `step_generator` builds a
            // StopIteration carrying its value (CPython's `e.value`, used by a
            // delegating `yield from`). Returns nested in a for/while/try
            // already propagate this way; a top-level return must match.
            Err(EvalError::Signal(ControlFlow::Return(v))) => {
                return Err(EvalError::Signal(ControlFlow::Return(v)));
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

async fn run_for_suspendable(
    state: &mut InterpreterState,
    node: &rustpython_parser::ast::StmtFor,
    target: &str,
    tools: &crate::tools::Tools,
) -> Result<(), EvalError> {
    use crate::eval::eval_expr;
    use crate::eval::functions::resolve_proxy;

    let (items, mut pos, mut body_index) = {
        let id = state.active_generator_stack.last().copied();
        if let Some(id) = id {
            if let Some(frame) = state.generators.get(&id) {
                if let Some(fs) = frame.for_stack.last() {
                    if fs.target == target {
                        (fs.items.clone(), fs.pos, fs.body_index)
                    } else {
                        let iterable = eval_expr(state, &node.iter, tools).await?;
                        let iterable = resolve_proxy(&iterable).await?;
                        let items = crate::eval::op::iter(state, &iterable, tools).await?;
                        (Arc::new(items), 0, 0)
                    }
                } else {
                    let iterable = eval_expr(state, &node.iter, tools).await?;
                    let iterable = resolve_proxy(&iterable).await?;
                    (Arc::new(crate::eval::op::iter(state, &iterable, tools).await?), 0, 0)
                }
            } else {
                let iterable = eval_expr(state, &node.iter, tools).await?;
                let iterable = resolve_proxy(&iterable).await?;
                (Arc::new(crate::eval::op::iter(state, &iterable, tools).await?), 0, 0)
            }
        } else {
            let iterable = eval_expr(state, &node.iter, tools).await?;
            let iterable = resolve_proxy(&iterable).await?;
            (Arc::new(crate::eval::op::iter(state, &iterable, tools).await?), 0, 0)
        }
    };

    while pos < items.len() {
        let item = items[pos].clone();
        if body_index == 0 {
            assign_target(state, &node.target, item, tools).await?;
        }
        while body_index < node.body.len() {
            let stmt = &node.body[body_index];
            match eval_stmt(state, stmt, tools).await {
                Ok(_) => {
                    body_index += 1;
                    if let Some(&id) = state.active_generator_stack.last() {
                        if let Some(frame) = state.generators.get_mut(&id) {
                            frame.resume_at_yield = false;
                        }
                    }
                }
                Err(EvalError::Signal(ControlFlow::Continue)) => {
                    body_index = node.body.len(); // end this iteration
                }
                Err(EvalError::Signal(ControlFlow::Break)) => {
                    if let Some(&id) = state.active_generator_stack.last() {
                        if let Some(frame) = state.generators.get_mut(&id) {
                            frame.for_stack.retain(|fs| fs.target != target);
                        }
                    }
                    return Ok(());
                }
                Err(EvalError::Signal(ControlFlow::Yield(v))) => {
                    if let Some(&id) = state.active_generator_stack.last() {
                        if let Some(frame) = state.generators.get_mut(&id) {
                            frame.resume_at_yield = true;
                            let entry = GeneratorForState {
                                items: items.clone(),
                                pos,
                                body_index, // re-enter this yield stmt
                                target: target.to_string(),
                            };
                            // Preserve yield-from drain frames on top.
                            if frame.for_stack.last().is_some_and(|t| t.target.is_empty()) {
                                let yf = frame.for_stack.pop();
                                if let Some(top) = frame.for_stack.last_mut() {
                                    if top.target == target {
                                        *top = entry;
                                    } else {
                                        frame.for_stack.push(entry);
                                    }
                                } else {
                                    frame.for_stack.push(entry);
                                }
                                if let Some(yf) = yf {
                                    frame.for_stack.push(yf);
                                }
                            } else if let Some(top) = frame.for_stack.last_mut() {
                                if top.target == target {
                                    *top = entry;
                                } else {
                                    frame.for_stack.push(entry);
                                }
                            } else {
                                frame.for_stack.push(entry);
                            }
                        }
                    }
                    return Err(EvalError::Signal(ControlFlow::Yield(v)));
                }
                Err(e) => return Err(e),
            }
        }
        // Finished body for this item.
        pos += 1;
        body_index = 0;
        if let Some(&id) = state.active_generator_stack.last() {
            if let Some(frame) = state.generators.get_mut(&id) {
                if let Some(top) = frame.for_stack.last_mut() {
                    if top.target == target {
                        top.pos = pos;
                        top.body_index = 0;
                    }
                }
            }
        }
    }

    if let Some(&id) = state.active_generator_stack.last() {
        if let Some(frame) = state.generators.get_mut(&id) {
            frame.for_stack.retain(|fs| fs.target != target);
        }
    }
    Ok(())
}

/// Run a top-level `while` loop inside a generator, suspending on `yield` and
/// resuming exactly where it left off. Mirrors `run_for_suspendable` but keyed
/// on a single `while_resume` body-index rather than a per-target stack — the
/// `generator_suspendable` gate guarantees the while is top-level with
/// direct-statement yields, so a single index is exact.
async fn run_while_suspendable(
    state: &mut InterpreterState,
    node: &rustpython_parser::ast::StmtWhile,
    tools: &crate::tools::Tools,
) -> Result<(), EvalError> {
    use crate::eval::eval_expr;

    // `while_resume == Some(i)` means we suspended on a yield at body statement
    // `i`; re-enter there and skip the condition check for that first pass.
    let clear_resume = |state: &mut InterpreterState| {
        if let Some(&id) = state.active_generator_stack.last() {
            if let Some(frame) = state.generators.get_mut(&id) {
                frame.while_resume = None;
            }
        }
    };
    let (mut body_index, mut resuming) = {
        let saved = state
            .active_generator_stack
            .last()
            .copied()
            .and_then(|id| state.generators.get(&id))
            .and_then(|frame| frame.while_resume);
        match saved {
            Some(i) => (i, true),
            None => (0, false),
        }
    };

    loop {
        if !resuming {
            let cond = eval_expr(state, &node.test, tools).await?;
            if !crate::eval::op::truthy(state, &cond, tools).await? {
                break;
            }
        }
        resuming = false;
        while body_index < node.body.len() {
            let stmt = &node.body[body_index];
            match eval_stmt(state, stmt, tools).await {
                Ok(_) => {
                    body_index += 1;
                    if let Some(&id) = state.active_generator_stack.last() {
                        if let Some(frame) = state.generators.get_mut(&id) {
                            frame.resume_at_yield = false;
                        }
                    }
                }
                Err(EvalError::Signal(ControlFlow::Continue)) => {
                    body_index = node.body.len(); // end this iteration, re-check
                }
                Err(EvalError::Signal(ControlFlow::Break)) => {
                    clear_resume(state);
                    return Ok(());
                }
                Err(EvalError::Signal(ControlFlow::Yield(v))) => {
                    if let Some(&id) = state.active_generator_stack.last() {
                        if let Some(frame) = state.generators.get_mut(&id) {
                            frame.resume_at_yield = true;
                            frame.while_resume = Some(body_index); // re-enter this stmt
                        }
                    }
                    return Err(EvalError::Signal(ControlFlow::Yield(v)));
                }
                Err(e) => return Err(e),
            }
        }
        body_index = 0; // iteration finished — re-evaluate the condition
    }
    clear_resume(state);
    Ok(())
}

/// Run a top-level `try` inside a generator, stepping the body
/// statement-by-statement so a `yield` suspends at its own statement
/// (tracked by `try_resume`) instead of re-running the whole body.
/// `else`/`except`/`finally` run once the body actually completes or
/// raises — never on a suspend. Direct-statement yields in the body are
/// the resumable case; a yield nested in the body's own compound
/// statements resumes at the enclosing body statement (the same
/// granularity `for`/`while` use).
fn run_try_suspendable<'a>(
    state: &'a mut InterpreterState,
    node: &'a rustpython_parser::ast::StmtTry,
    tools: &'a crate::tools::Tools,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EvalError>> + Send + 'a>> {
    Box::pin(run_try_suspendable_inner(state, node, tools))
}

async fn run_try_suspendable_inner(
    state: &mut InterpreterState,
    node: &rustpython_parser::ast::StmtTry,
    tools: &crate::tools::Tools,
) -> Result<(), EvalError> {
    use crate::state::{TryPhase, TryResume};

    // Pop this nesting level's resume position (LIFO): the outermost try
    // is resumed first and pops the top; recursing into a nested try pops
    // the next. An empty stack means a fresh (non-resuming) execution.
    let resume = pop_try_resume(state);

    // Phase 1 — BODY (skip when resuming inside a later phase).
    if resume.is_none_or(|r| matches!(r.phase, TryPhase::Body)) {
        let start = resume.map_or(0, |r| r.index);
        match step_try_phase(state, &node.body, start, TryPhase::Body, tools).await {
            TryStep::Yielded(v) => return Err(EvalError::Signal(ControlFlow::Yield(v))),
            TryStep::Completed => {
                // try succeeded → run `else`, then fall to finally.
                match step_try_phase(state, &node.orelse, 0, TryPhase::Orelse, tools).await {
                    TryStep::Yielded(v) => {
                        return Err(EvalError::Signal(ControlFlow::Yield(v)));
                    }
                    TryStep::Completed => {
                        return finish_try(state, &node.finalbody, None, tools).await;
                    }
                    TryStep::Errored(e) => {
                        return finish_try(state, &node.finalbody, Some(e), tools).await;
                    }
                }
            }
            TryStep::Errored(err) => {
                // Body raised: match a handler and enter it, else propagate.
                return enter_or_propagate(state, node, err, tools).await;
            }
        }
    }

    // Phase 2 — resume inside a matched HANDLER.
    if let Some(TryResume { phase: TryPhase::Handler(h), index }) = resume {
        let handler = &node.handlers[h];
        let rustpython_parser::ast::ExceptHandler::ExceptHandler(h_node) = handler;
        match step_try_phase(state, &h_node.body, index, TryPhase::Handler(h), tools).await {
            TryStep::Yielded(v) => return Err(EvalError::Signal(ControlFlow::Yield(v))),
            TryStep::Completed => {
                cleanup_handler(state, h_node);
                return finish_try(state, &node.finalbody, None, tools).await;
            }
            TryStep::Errored(e) => {
                cleanup_handler(state, h_node);
                return finish_try(state, &node.finalbody, Some(e), tools).await;
            }
        }
    }

    // Phase 3 — resume inside ORELSE or FINALLY.
    match resume {
        Some(TryResume { phase: TryPhase::Orelse, index }) => {
            match step_try_phase(state, &node.orelse, index, TryPhase::Orelse, tools).await {
                TryStep::Yielded(v) => Err(EvalError::Signal(ControlFlow::Yield(v))),
                TryStep::Completed => finish_try(state, &node.finalbody, None, tools).await,
                TryStep::Errored(e) => finish_try(state, &node.finalbody, Some(e), tools).await,
            }
        }
        Some(TryResume { phase: TryPhase::Finally, index }) => {
            match step_try_phase(state, &node.finalbody, index, TryPhase::Finally, tools).await {
                TryStep::Yielded(v) => Err(EvalError::Signal(ControlFlow::Yield(v))),
                TryStep::Completed => Ok(()),
                TryStep::Errored(e) => Err(e),
            }
        }
        // Body/Handler already handled by the earlier phases.
        _ => Ok(()),
    }
}

/// Outcome of stepping one phase of a suspendable `try`.
enum TryStep {
    Completed,
    Yielded(Box<Value>),
    Errored(EvalError),
}

/// Step the statements of one `try` phase from `start`; on a `yield`,
/// record the resume position `(phase, index)` and return `Yielded`.
async fn step_try_phase(
    state: &mut InterpreterState,
    stmts: &[Stmt],
    start: usize,
    phase: crate::state::TryPhase,
    tools: &crate::tools::Tools,
) -> TryStep {
    let mut i = start;
    while i < stmts.len() {
        // A nested `try` is stepped through the suspend engine too, so a
        // yield deep inside nested trys resumes correctly (its resume
        // position is pushed by the recursive call). A direct statement
        // runs via `eval_stmt` and, if it yields, sets `resume_at_yield`
        // so re-entering that statement delivers the sent value.
        let (result, nested) = if let Stmt::Try(nested_try) = &stmts[i] {
            (run_try_suspendable(state, nested_try, tools).await.map(|()| Value::None), true)
        } else {
            (eval_stmt(state, &stmts[i], tools).await, false)
        };
        match result {
            Ok(_) => {
                i += 1;
                clear_resume_at_yield(state);
            }
            Err(EvalError::Signal(ControlFlow::Yield(v))) => {
                // Record this level's resume position on the stack.
                push_try_resume(state, crate::state::TryResume { phase, index: i });
                // The innermost direct yield already set `resume_at_yield`;
                // a nested-try yield must not clear/overwrite it.
                if !nested {
                    set_resume_at_yield(state);
                }
                return TryStep::Yielded(v);
            }
            Err(other) => return TryStep::Errored(other),
        }
    }
    TryStep::Completed
}

fn clear_resume_at_yield(state: &mut InterpreterState) {
    if let Some(&id) = state.active_generator_stack.last() {
        if let Some(frame) = state.generators.get_mut(&id) {
            frame.resume_at_yield = false;
        }
    }
}

fn set_resume_at_yield(state: &mut InterpreterState) {
    if let Some(&id) = state.active_generator_stack.last() {
        if let Some(frame) = state.generators.get_mut(&id) {
            frame.resume_at_yield = true;
        }
    }
}

fn push_try_resume(state: &mut InterpreterState, resume: crate::state::TryResume) {
    if let Some(&id) = state.active_generator_stack.last() {
        if let Some(frame) = state.generators.get_mut(&id) {
            frame.try_stack.push(resume);
        }
    }
}

fn pop_try_resume(state: &mut InterpreterState) -> Option<crate::state::TryResume> {
    let id = *state.active_generator_stack.last()?;
    state.generators.get_mut(&id)?.try_stack.pop()
}

/// A body exception: bind + enter the first matching handler (stepping
/// it so a yield inside suspends), or propagate after finally.
async fn enter_or_propagate(
    state: &mut InterpreterState,
    node: &rustpython_parser::ast::StmtTry,
    err: EvalError,
    tools: &crate::tools::Tools,
) -> Result<(), EvalError> {
    use crate::eval::exceptions::{interpreter_error_to_exception_pub, matches_handler};
    // Signals (break/continue/return) are not caught by `except`.
    let exc = match &err {
        EvalError::Exception(e) => e.clone(),
        EvalError::Interpreter(ie) => interpreter_error_to_exception_pub(ie),
        EvalError::Signal(_) => return finish_try(state, &node.finalbody, Some(err), tools).await,
    };
    for (h, handler) in node.handlers.iter().enumerate() {
        let rustpython_parser::ast::ExceptHandler::ExceptHandler(h_node) = handler;
        if !matches_handler(state, &exc, h_node, tools).await? {
            continue;
        }
        // Bind the exception name and push it so a bare `raise` re-raises.
        if let Some(name) = &h_node.name {
            state
                .set_variable(name.as_str(), Value::Exception(Box::new(exc.clone())))
                .map_err(EvalError::Interpreter)?;
        }
        state.active_exception_stack.push(exc.clone());
        return match step_try_phase(state, &h_node.body, 0, TryPhase::Handler(h), tools).await {
            TryStep::Yielded(v) => Err(EvalError::Signal(ControlFlow::Yield(v))),
            TryStep::Completed => {
                cleanup_handler(state, h_node);
                finish_try(state, &node.finalbody, None, tools).await
            }
            TryStep::Errored(e) => {
                cleanup_handler(state, h_node);
                finish_try(state, &node.finalbody, Some(e), tools).await
            }
        };
    }
    // No handler matched — run finally then re-raise.
    finish_try(state, &node.finalbody, Some(err), tools).await
}

/// Unbind an except handler's variable and pop the active-exception
/// stack, once the handler body has finished (matches CPython teardown).
fn cleanup_handler(
    state: &mut InterpreterState,
    h_node: &rustpython_parser::ast::ExceptHandlerExceptHandler,
) {
    state.active_exception_stack.pop();
    if let Some(name) = &h_node.name {
        let _ = state.delete_variable(name.as_str());
    }
}

use crate::state::TryPhase;

/// Run the `finally` block and produce the try's final result: a fresh
/// error in `finally` overrides `pending`; a `yield` in `finally`
/// suspends. Clears the suspend marker on real completion.
async fn finish_try(
    state: &mut InterpreterState,
    finalbody: &[Stmt],
    pending: Option<EvalError>,
    tools: &crate::tools::Tools,
) -> Result<(), EvalError> {
    // This level's resume position was already popped on entry, so a
    // clean completion leaves nothing to clear.
    match step_try_phase(state, finalbody, 0, TryPhase::Finally, tools).await {
        TryStep::Yielded(v) => Err(EvalError::Signal(ControlFlow::Yield(v))),
        TryStep::Completed => pending.map_or(Ok(()), Err),
        TryStep::Errored(e) => Err(e),
    }
}

async fn resume_for(
    state: &mut InterpreterState,
    tools: &crate::tools::Tools,
) -> Result<(), EvalError> {
    // Re-enter the current top-level for statement via stmt_index.
    let (body, stmt_index) = {
        let id = state
            .active_generator_stack
            .last()
            .copied()
            .ok_or_else(|| InterpreterError::Runtime("no active generator".into()))?;
        let frame = state
            .generators
            .get(&id)
            .ok_or_else(|| InterpreterError::Runtime("generator frame missing".into()))?;
        (frame.body.clone(), frame.stmt_index)
    };
    if stmt_index >= body.len() {
        return Ok(());
    }
    if let Stmt::For(for_node) = &body[stmt_index] {
        if let rustpython_parser::ast::Expr::Name(t) = for_node.target.as_ref() {
            let target = t.id.as_str().to_string();
            return run_for_suspendable(state, for_node, &target, tools).await;
        }
    }
    Ok(())
}

/// Single-step a `Value::BuiltinIter` (the infinite `itertools`
/// producers). `__iter__` returns the iterator itself; `__next__`
/// advances the cursor, raising `StopIteration` only for an exhausted
/// (empty) `cycle`.
fn dispatch_builtin_iter(
    state: &mut InterpreterState,
    receiver: &Value,
    id: u64,
    method: &str,
    args: &[Value],
) -> EvalResult {
    match method {
        "__iter__" => Ok(receiver.clone()),
        "__next__" => {
            if !args.is_empty() {
                return Err(
                    InterpreterError::TypeError("__next__() takes no arguments".into()).into()
                );
            }
            state.step_builtin_iter(id).ok_or_else(|| {
                EvalError::Exception(crate::value::ExceptionValue::new("StopIteration", ""))
            })
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'{}' object has no attribute '{method}'",
            receiver.type_name()
        ))
        .into()),
    }
}

fn dispatch_lazy(
    state: &mut InterpreterState,
    receiver: &Value,
    method: &str,
    args: &[Value],
) -> EvalResult {
    let Value::Lazy { items, cursor_id } = receiver else {
        return Err(InterpreterError::TypeError(format!(
            "'{}' object has no attribute '{method}'",
            receiver.type_name()
        ))
        .into());
    };
    let cursor = state.lazy_cursors.get(cursor_id).copied().unwrap_or(0);

    match method {
        "__next__" | "send" => {
            if method == "send" {
                let value = args.first().cloned().unwrap_or(Value::None);
                if cursor == 0 && !matches!(value, Value::None) {
                    return Err(InterpreterError::TypeError(
                        "can't send non-None value to a just-started generator".into(),
                    )
                    .into());
                }
            } else if !args.is_empty() {
                return Err(
                    InterpreterError::TypeError("__next__() takes no arguments".into()).into()
                );
            }
            if cursor < items.len() {
                state.lazy_cursors.insert(*cursor_id, cursor + 1);
                Ok(items[cursor].clone())
            } else {
                Err(EvalError::Exception(ExceptionValue::new("StopIteration", String::new())))
            }
        }
        "throw" => {
            state.lazy_cursors.insert(*cursor_id, items.len());
            let (type_name, message) = match args.first() {
                Some(Value::Exception(e)) => (e.type_name.clone(), e.message.clone()),
                Some(Value::ExceptionType(n)) => {
                    let msg = args.get(1).map(|v| format!("{v}")).unwrap_or_default();
                    (n.clone(), msg)
                }
                Some(other) => (
                    "TypeError".into(),
                    format!(
                        "exceptions must derive from BaseException, not '{}'",
                        other.type_name()
                    ),
                ),
                None => {
                    return Err(InterpreterError::TypeError(
                        "throw() takes at least 1 argument".into(),
                    )
                    .into());
                }
            };
            Err(EvalError::Exception(ExceptionValue::new(type_name, message)))
        }
        "close" => {
            state.lazy_cursors.insert(*cursor_id, items.len());
            Ok(Value::None)
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'generator' object has no attribute '{method}'"
        ))
        .into()),
    }
}
