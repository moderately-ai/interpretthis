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
            stmt_index: 0,
            for_stack: Vec::new(),
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
        "throw" => {
            // Mark finished and raise the requested exception into the caller.
            if let Some(frame) = state.generators.get_mut(&id) {
                frame.finished = true;
                frame.closed = true;
            }
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
            if let Some(frame) = state.generators.get_mut(&id) {
                frame.finished = true;
                frame.closed = true;
                frame.for_stack.clear();
            }
            Ok(Value::None)
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'generator' object has no attribute '{method}'"
        ))
        .into()),
    }
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
        Err(EvalError::Signal(ControlFlow::Return(_))) => {
            if let Some(frame) = state.generators.get_mut(&id) {
                frame.finished = true;
            }
            Err(EvalError::Exception(ExceptionValue::new("StopIteration", String::new())))
        }
        Err(e) => {
            if let Some(frame) = state.generators.get_mut(&id) {
                frame.finished = true;
            }
            Err(e)
        }
    }
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
            Err(EvalError::Signal(ControlFlow::Return(_))) => return Ok(()),
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
                        (items, 0, 0)
                    }
                } else {
                    let iterable = eval_expr(state, &node.iter, tools).await?;
                    let iterable = resolve_proxy(&iterable).await?;
                    (crate::eval::op::iter(state, &iterable, tools).await?, 0, 0)
                }
            } else {
                let iterable = eval_expr(state, &node.iter, tools).await?;
                let iterable = resolve_proxy(&iterable).await?;
                (crate::eval::op::iter(state, &iterable, tools).await?, 0, 0)
            }
        } else {
            let iterable = eval_expr(state, &node.iter, tools).await?;
            let iterable = resolve_proxy(&iterable).await?;
            (crate::eval::op::iter(state, &iterable, tools).await?, 0, 0)
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
