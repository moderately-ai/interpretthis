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

/// True when `method` is a generator-iterator protocol name — the methods the
/// generator/lazy/builtin-iter dispatchers own (rather than falling through to
/// generic attribute lookup). `__iter__` is included because an iterator is its
/// own iterator, so `g.__iter__()` must return the iterator itself.
#[must_use]
pub(crate) fn is_generator_method(method: &str) -> bool {
    matches!(method, "send" | "throw" | "close" | "__next__" | "__iter__")
}

/// Whether a generator body can use the suspend path: every top-level statement
/// must be `top_level_suspendable`. Anything the resume steppers cannot re-enter
/// exactly (a `while` with an `else`, a yield buried in a nested loop/`with`,
/// a tuple-target `for`) forces the eager Lazy buffer, which is correct for
/// finite generators.
#[must_use]
pub(crate) fn generator_suspendable(stmts: &[Stmt]) -> bool {
    stmts.iter().all(top_level_suspendable)
}

/// Whether a single top-level generator-body statement can suspend correctly.
/// A statement with no `yield` is always fine; a yielding one must use only the
/// shapes the top-level steppers resume exactly: direct yields, a name-target
/// `for` (`run_for_suspendable`), a `while` (`run_while_suspendable`), an `if`
/// (`run_if_suspendable`), or a `try` (`run_try_suspendable`) — each with a body
/// its stepper can re-enter. Loop bodies use `loop_body_suspendable` (they step
/// only through nested `if`); `if`/`try` bodies use `if_branch_suspendable`
/// (they also step through nested `try`). Anything else — a yield inside a
/// nested loop/`with`/`match`, a tuple-target `for`, a `for`/`while` with an
/// `else` — falls back to the eager buffer, correct for finite generators.
fn top_level_suspendable(stmt: &Stmt) -> bool {
    use rustpython_parser::ast::ExceptHandler;
    if !super::definitions::contains_yield_stmts(std::slice::from_ref(stmt)) {
        return true;
    }
    match stmt {
        // A top-level `for`/`while` may nest further loops: any depth of `for`s
        // (each keys its own `for_stack` frame) plus at most one `while`. A
        // top-level `while` consumes that single while-budget.
        Stmt::For(f) => {
            for_name_target(f).is_some() && f.orelse.is_empty() && loop_nest_suspendable(&f.body)
        }
        Stmt::While(w) => w.orelse.is_empty() && loop_nest_suspendable(&w.body),
        Stmt::If(n) => if_branch_suspendable(&n.body) && if_branch_suspendable(&n.orelse),
        Stmt::Try(t) => {
            if_branch_suspendable(&t.body)
                && if_branch_suspendable(&t.orelse)
                && if_branch_suspendable(&t.finalbody)
                && t.handlers.iter().all(|h| {
                    let ExceptHandler::ExceptHandler(eh) = h;
                    if_branch_suspendable(&eh.body)
                })
        }
        // A `with` whose body can suspend is stepped by `run_with_suspendable`,
        // which enters the context managers once and defers `__exit__` to the
        // real block exit — so a `@contextmanager` generator can `yield` from
        // inside a `with`.
        Stmt::With(w) => if_branch_suspendable(&w.body),
        // A bare `yield` statement, or one whose RHS/return value yields.
        Stmt::Expr(_)
        | Stmt::Assign(_)
        | Stmt::AugAssign(_)
        | Stmt::AnnAssign(_)
        | Stmt::Return(_) => true,
        // match / async / anything else carrying a yield → eager.
        _ => false,
    }
}

/// A top-level loop body the suspend engine can re-enter, to ANY depth of nested
/// loops. A nested `for` (bare-name target, no `else`) or `while` (no `else`) is
/// allowed when its own body is itself loop-nest-suspendable — each `for` keys
/// its `for_stack` frame by target and each `while` its `while_resume` entry by
/// AST offset, so every loop in the nest keeps a distinct live frame
/// (`for i: for j: for k: yield`, `while a: while b: yield`, any mix). Only
/// `with`/`try`/`match` carrying a yield inside a loop, or a tuple-target /
/// `else`-clause loop, force the eager buffer. Mirrors exactly what the loop
/// steppers' `Stmt::For`/`Stmt::While` arms delegate.
fn loop_nest_suspendable(stmts: &[Stmt]) -> bool {
    stmts.iter().all(|stmt| {
        let yields = super::definitions::contains_yield_stmts(std::slice::from_ref(stmt));
        match stmt {
            Stmt::If(node) => {
                loop_nest_suspendable(&node.body) && loop_nest_suspendable(&node.orelse)
            }
            Stmt::For(f) => !yields || (for_delegatable(f) && loop_nest_suspendable(&f.body)),
            Stmt::While(w) => !yields || (while_delegatable(w) && loop_nest_suspendable(&w.body)),
            Stmt::With(_) | Stmt::Try(_) | Stmt::TryStar(_) | Stmt::Match(_) => !yields,
            _ => true,
        }
    })
}

/// The bare-name loop target of a `for`, or `None` for a tuple/attribute
/// target. `run_for_suspendable` tracks resume state per target name, so only a
/// bare-name `for` can be suspended; other shapes fall back to the eager buffer.
fn for_name_target(f: &rustpython_parser::ast::StmtFor) -> Option<&str> {
    match f.target.as_ref() {
        rustpython_parser::ast::Expr::Name(t) => Some(t.id.as_str()),
        _ => None,
    }
}

/// A `for` whose SHAPE the suspend engine can step (bare-name target, no
/// `else`) — used by the loop steppers' delegation arms. Unlike
/// `for_loop_suspendable` it does NOT constrain the body, because arbitrary loop
/// nesting is supported: each `for` keys its own `for_stack` frame by target, so
/// `for i: for j: for k: yield` keeps three frames live. The gate
/// (`loop_nest_suspendable`) validates the whole nest before the suspend path is
/// taken, so the stepper arm only needs the shape.
fn for_delegatable(f: &rustpython_parser::ast::StmtFor) -> bool {
    for_name_target(f).is_some() && f.orelse.is_empty()
}

/// A `while` whose shape the suspend engine can step (no `else`). Each `while`
/// keys its own `while_resume` entry by AST offset, so arbitrary while nesting
/// is supported; the gate validates the nest.
fn while_delegatable(w: &rustpython_parser::ast::StmtWhile) -> bool {
    w.orelse.is_empty()
}

/// Yields inside an `if` branch or `try` phase can be suspended by the `if`
/// stepper (`run_if_suspendable`) and the `try` stepper (`step_try_phase`),
/// which both re-enter a nested `if` *and* a nested `try` exactly. So yields may
/// sit in direct statements, nested `if`s, or nested `try`s — but not inside a
/// `for`/`while`/`with`/`match`/`try*`, which those steppers re-run from the top.
fn if_branch_suspendable(stmts: &[Stmt]) -> bool {
    use rustpython_parser::ast::ExceptHandler;
    stmts.iter().all(|stmt| match stmt {
        Stmt::If(node) => if_branch_suspendable(&node.body) && if_branch_suspendable(&node.orelse),
        Stmt::Try(t) => {
            if_branch_suspendable(&t.body)
                && if_branch_suspendable(&t.orelse)
                && if_branch_suspendable(&t.finalbody)
                && t.handlers.iter().all(|h| {
                    let ExceptHandler::ExceptHandler(eh) = h;
                    if_branch_suspendable(&eh.body)
                })
        }
        // `step_try_phase`/`step_with_body`/`run_if_suspendable` step a
        // yield-bearing `while` or `for` through its own loop stepper (see the
        // arms there), so either may carry a yield as long as it has a
        // suspendable shape. The `for`'s position-based resume survives the
        // parent re-entry because the parent's resume record routes back into
        // `run_for_suspendable`, which restores its slot from `for_stack`.
        // A nested loop is stepped by its own suspend engine. Its BODY must be
        // `loop_nest_suspendable` (deep loop nesting is fine — each loop keys its
        // own resume frame — but a `try`/`with` inside a loop body cannot be
        // stepped by the loop stepper, so those stay eager). The loop itself is
        // reachable here through the `if`/`try` steppers' delegation arms.
        Stmt::While(w) => {
            !super::definitions::contains_yield_stmts(std::slice::from_ref(stmt))
                || (while_delegatable(w) && loop_nest_suspendable(&w.body))
        }
        Stmt::For(f) => {
            !super::definitions::contains_yield_stmts(std::slice::from_ref(stmt))
                || (for_delegatable(f) && loop_nest_suspendable(&f.body))
        }
        Stmt::With(_) | Stmt::TryStar(_) | Stmt::Match(_) => {
            !super::definitions::contains_yield_stmts(std::slice::from_ref(stmt))
        }
        _ => true,
    })
}

/// Create a suspended generator from a just-bound function frame.
/// Build a lazy generator from a synthesized `body` plus the frame `locals` it
/// reads (`touched` names are written back on suspend). Returns `None` when the
/// body cannot be driven by the suspend engine (caller falls back to eager
/// materialisation). Shared by lazy generator expressions and lazy
/// `itertools.islice`.
pub(crate) fn create_synthetic_generator(
    state: &mut InterpreterState,
    name: &str,
    body: Arc<Vec<Stmt>>,
    locals: rustc_hash::FxHashMap<String, Value>,
    touched: Vec<String>,
) -> Option<Value> {
    if !generator_suspendable(&body) {
        return None;
    }
    let func_def = FunctionDef {
        name: name.to_string(),
        body_key: format!("{name}#{}", state.next_cursor_id),
        wraps_name: None,
        params: crate::value::FunctionParams {
            args: Vec::new(),
            defaults: Vec::new(),
            default_values: Vec::new(),
            vararg: None,
            kwonlyargs: Vec::new(),
            kw_defaults: Vec::new(),
            kw_default_values: Vec::new(),
            kwarg: None,
            posonly_count: 0,
        },
        closure: std::collections::BTreeMap::new(),
        source: String::new(),
        nonlocal_names: Vec::new(),
        is_generator: true,
        nonlocal_cell_id: None,
        assigned_names: Vec::new(),
        global_names: Vec::new(),
        is_module_level: state.call_depth == 0,
        docstring: None,
        cell_refreshes: Vec::new(),
        qualname: String::new(),
        annotations: Vec::new(),
        is_async: false,
    };
    Some(create_generator(state, &func_def, body, locals, touched))
}

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
            while_resume: Vec::new(),
            try_stack: Vec::new(),
            if_stack: Vec::new(),
            with_stack: Vec::new(),
            yield_from_return: None,
            delegating_to: None,
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

/// Forward a `next`/`send`/`throw` on a delegating generator (`id`) to the
/// sub-generator it is suspended inside via `yield from` (`sub_id`). While the
/// sub still yields, the delegating generator stays parked. When the sub
/// finishes (StopIteration) or raises, the delegating generator resumes its own
/// body past the `yield from` — with the sub's return value, or re-raising the
/// sub's exception at the yield-from point.
async fn forward_to_delegate(
    state: &mut InterpreterState,
    id: u64,
    sub_id: u64,
    method: &str,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let sub_method = if method == "throw" { "throw" } else { method };
    // Boxed: dispatch_generator_method -> dispatch_suspended -> here is an
    // async recursion cycle.
    let sub_result = Box::pin(dispatch_generator_method(
        state,
        &Value::Generator { id: sub_id },
        sub_method,
        args,
        &indexmap::IndexMap::new(),
        tools,
    ))
    .await;
    match sub_result {
        // Sub yielded again — remain delegating.
        Ok(v) => Ok(v),
        // Sub exhausted — resume our body past the yield-from with its return.
        Err(EvalError::Exception(e)) if e.type_name == "StopIteration" => {
            let ret = e.args.first().cloned().unwrap_or(Value::None);
            if let Some(frame) = state.generators.get_mut(&id) {
                frame.delegating_to = None;
                frame.yield_from_return = Some(ret);
                frame.resume_at_yield = true;
            }
            step_generator(state, id, Value::None, tools).await
        }
        // Sub raised — propagate the exception at the yield-from point so a
        // try/except around the `yield from` in our body can catch it.
        Err(EvalError::Exception(e)) => {
            if let Some(frame) = state.generators.get_mut(&id) {
                frame.delegating_to = None;
                frame.pending_throw = Some(Box::new(e));
                frame.resume_at_yield = true;
            }
            step_generator(state, id, Value::None, tools).await
        }
        Err(other) => Err(other),
    }
}

async fn dispatch_suspended(
    state: &mut InterpreterState,
    id: u64,
    method: &str,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    // While suspended inside `yield from <generator>`, forward next/send/throw
    // to the delegated sub-generator so it stays lazy (its `finally` runs when
    // it is actually exhausted / closed, not eagerly).
    if matches!(method, "__next__" | "send" | "throw") {
        let delegating_to = state.generators.get(&id).and_then(|f| f.delegating_to);
        if let Some(sub_id) = delegating_to {
            return forward_to_delegate(state, id, sub_id, method, args, tools).await;
        }
    }
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
            // Closing a generator suspended in `yield from <sub>` closes the
            // sub first (running its `finally`), then continues to close this
            // generator so any `finally` around the yield-from also runs.
            if let Some(sub_id) = state.generators.get(&id).and_then(|f| f.delegating_to) {
                let _ = Box::pin(dispatch_generator_method(
                    state,
                    &Value::Generator { id: sub_id },
                    "close",
                    &[],
                    &indexmap::IndexMap::new(),
                    tools,
                ))
                .await?;
                if let Some(frame) = state.generators.get_mut(&id) {
                    frame.delegating_to = None;
                }
            }
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
        // A generator is its own iterator.
        "__iter__" => Ok(Value::Generator { id }),
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
        frame.with_stack.clear();
        frame.while_resume.clear();
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
    // A suspended top-level `for` leaves `stmt_index` pointing at the `for`
    // statement itself, so the `Stmt::For` arm below re-enters and resumes it
    // via `for_stack` — then advances to the statements after the loop. A `for`
    // suspended inside a `try`/`if`/`with` is resumed through that parent
    // stepper (its resume record routes back into `run_for_suspendable`), not
    // here.
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
        // Top-level `with` containing a (suspendable) yield: step it so the
        // context managers are entered once and `__exit__` runs at the real
        // block exit rather than on each yield (see `run_with_suspendable`).
        if let Stmt::With(with_node) = stmt {
            if if_branch_suspendable(&with_node.body)
                && super::definitions::contains_yield_stmts(std::slice::from_ref(stmt))
            {
                match run_with_suspendable(state, with_node, tools).await {
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
        // Top-level `if` containing a (suspendable) yield: step it so a yield
        // after side-effecting statements resumes at the yield, not the top.
        if let Stmt::If(if_node) = stmt {
            if if_branch_suspendable(&if_node.body)
                && if_branch_suspendable(&if_node.orelse)
                && super::definitions::contains_yield_stmts(std::slice::from_ref(stmt))
            {
                match run_if_suspendable(state, if_node, tools).await {
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

/// Insert or replace the `for_stack` resume frame for `entry.target`. A frame
/// already recorded for that target is updated in place (so a suspended loop
/// keeps ONE slot); a fresh target is inserted below any trailing yield-from
/// drain frames (empty target), which must stay on top for `step_generator`'s
/// drain. Keying by target — rather than assuming the top-of-stack is ours —
/// is what lets two `for` loops (`for i in …: for j in …: yield`) each keep a
/// distinct live frame simultaneously.
fn upsert_for_frame(for_stack: &mut Vec<GeneratorForState>, entry: GeneratorForState) {
    if let Some(slot) = for_stack.iter_mut().find(|fs| fs.target == entry.target) {
        *slot = entry;
        return;
    }
    let insert_at = for_stack.iter().rposition(|fs| !fs.target.is_empty()).map_or(0, |i| i + 1);
    for_stack.insert(insert_at, entry);
}

async fn run_for_suspendable(
    state: &mut InterpreterState,
    node: &rustpython_parser::ast::StmtFor,
    target: &str,
    tools: &crate::tools::Tools,
) -> Result<(), EvalError> {
    use crate::eval::eval_expr;
    use crate::eval::functions::resolve_proxy;

    // Resume state saved on the for_stack for THIS target, if any. Cloned out so
    // the immutable borrow ends before the `eval_expr` below (which needs `&mut`).
    let resume = {
        let id = state.active_generator_stack.last().copied();
        id.and_then(|id| state.generators.get(&id))
            // Search by target (topmost-first), not just the top — a nested
            // inner `for`'s frame may sit above this loop's frame.
            .and_then(|frame| frame.for_stack.iter().rev().find(|fs| fs.target == target))
            .map(|fs| {
                (
                    fs.lazy_source.clone(),
                    fs.current_item.clone(),
                    fs.items.clone(),
                    fs.pos,
                    fs.body_index,
                )
            })
    };
    let (items, mut pos, mut body_index) = match resume {
        // Resuming a lazy-source loop — continue pulling from the source.
        Some((Some(source), current_item, _, _, bi)) => {
            return run_for_lazy(state, node, target, source, current_item, bi, tools).await;
        }
        // Resuming a materialised loop.
        Some((None, _, items, pos, bi)) => (items, pos, bi),
        // Fresh entry: evaluate the source. A lazy source (generator / lazy
        // iterator / count-cycle-repeat) is stepped one item at a time so an
        // early `break` / `return` doesn't drain — or hang on — it.
        None => {
            let iterable = resolve_proxy(&eval_expr(state, &node.iter, tools).await?).await?;
            if matches!(
                iterable,
                Value::Generator { .. } | Value::Lazy { .. } | Value::BuiltinIter { .. }
            ) {
                return run_for_lazy(state, node, target, iterable, None, 0, tools).await;
            }
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
            let stmt_yields = super::definitions::contains_yield_stmts(std::slice::from_ref(stmt));
            // An `if` steps through `run_if_suspendable` so a yield inside its
            // branch resumes at the yield (via `if_stack`), not the branch top.
            // A nested `while` steps through `run_while_suspendable` (its
            // `while_resume` is disjoint from this loop's `for_stack`), so
            // `for i in …: while c: yield` suspends correctly.
            let (result, nested) = match stmt {
                Stmt::If(if_node)
                    if stmt_yields
                        && loop_nest_suspendable(&if_node.body)
                        && loop_nest_suspendable(&if_node.orelse) =>
                {
                    (run_if_suspendable(state, if_node, tools).await.map(|()| Value::None), true)
                }
                Stmt::While(w) if stmt_yields && while_delegatable(w) => {
                    // Boxed: `run_for` ↔ `run_while` is a mutual async recursion.
                    (
                        Box::pin(run_while_suspendable(state, w, tools))
                            .await
                            .map(|()| Value::None),
                        true,
                    )
                }
                // A nested `for` keys its own `for_stack` frame by target, so
                // `for i in …: for j in …: yield` keeps both live at once.
                Stmt::For(inner) if stmt_yields && for_delegatable(inner) => {
                    let inner_target = for_name_target(inner).unwrap_or_default().to_string();
                    // Boxed: direct async self-recursion.
                    (
                        Box::pin(run_for_suspendable(state, inner, &inner_target, tools))
                            .await
                            .map(|()| Value::None),
                        true,
                    )
                }
                _ => (eval_stmt(state, stmt, tools).await, false),
            };
            match result {
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
                            // A nested `if`/`while` stepper recorded its own
                            // resume; only a direct yield needs `resume_at_yield`.
                            if !nested {
                                frame.resume_at_yield = true;
                            }
                            let entry = GeneratorForState {
                                items: items.clone(),
                                pos,
                                body_index, // re-enter this yield stmt
                                target: target.to_string(),
                                lazy_source: None,
                                current_item: None,
                            };
                            upsert_for_frame(&mut frame.for_stack, entry);
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
                if let Some(slot) = frame.for_stack.iter_mut().find(|fs| fs.target == target) {
                    slot.pos = pos;
                    slot.body_index = 0;
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

/// The lazy variant of [`run_for_suspendable`] for a generator / lazy-iterator
/// source: pull ONE item at a time via `__next__` rather than materialising the
/// whole (possibly infinite) source. The source's own cursor tracks position, so
/// only the item currently being processed is remembered across a `yield`. This
/// is what makes `for x in some_generator: ...; break` stop early instead of
/// draining — or hanging on — the source, and gives loop-variable closures their
/// CPython interleaved-capture timing.
async fn run_for_lazy(
    state: &mut InterpreterState,
    node: &rustpython_parser::ast::StmtFor,
    target: &str,
    source: Value,
    mut current_item: Option<Value>,
    mut body_index: usize,
    tools: &crate::tools::Tools,
) -> Result<(), EvalError> {
    let empty = IndexMap::new();
    // On resume (first turn only) we continue the item already in progress at the
    // saved `body_index` — do NOT pull or re-bind, or a yield that was the FIRST
    // body statement would skip the next item. Every later turn pulls a fresh one.
    let mut resuming = current_item.is_some();
    loop {
        if resuming {
            resuming = false;
        } else {
            let next =
                Box::pin(dispatch_generator_method(state, &source, "__next__", &[], &empty, tools))
                    .await;
            let item = match next {
                Ok(v) => v,
                Err(EvalError::Exception(e)) if e.type_name == "StopIteration" => break,
                Err(e) => return Err(e),
            };
            assign_target(state, &node.target, item.clone(), tools).await?;
            current_item = Some(item);
            body_index = 0;
        }
        while body_index < node.body.len() {
            let stmt = &node.body[body_index];
            let stmt_yields = super::definitions::contains_yield_stmts(std::slice::from_ref(stmt));
            // Nested `if`/`for`/`while` step through their own suspend engines,
            // exactly as in `run_for_suspendable` — so `for i in count(): for j
            // in …: yield` (infinite outer) keeps both loops live.
            let (result, nested) = match stmt {
                Stmt::If(if_node)
                    if stmt_yields
                        && loop_nest_suspendable(&if_node.body)
                        && loop_nest_suspendable(&if_node.orelse) =>
                {
                    (run_if_suspendable(state, if_node, tools).await.map(|()| Value::None), true)
                }
                Stmt::While(w) if stmt_yields && while_delegatable(w) => (
                    Box::pin(run_while_suspendable(state, w, tools)).await.map(|()| Value::None),
                    true,
                ),
                Stmt::For(inner) if stmt_yields && for_delegatable(inner) => {
                    let inner_target = for_name_target(inner).unwrap_or_default().to_string();
                    (
                        Box::pin(run_for_suspendable(state, inner, &inner_target, tools))
                            .await
                            .map(|()| Value::None),
                        true,
                    )
                }
                _ => (eval_stmt(state, stmt, tools).await, false),
            };
            match result {
                Ok(_) => {
                    body_index += 1;
                    if let Some(&id) = state.active_generator_stack.last() {
                        if let Some(frame) = state.generators.get_mut(&id) {
                            frame.resume_at_yield = false;
                        }
                    }
                }
                Err(EvalError::Signal(ControlFlow::Continue)) => {
                    body_index = node.body.len();
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
                            if !nested {
                                frame.resume_at_yield = true;
                            }
                            let entry = GeneratorForState {
                                items: Arc::new(Vec::new()),
                                pos: 0,
                                body_index, // re-enter this yield stmt
                                target: target.to_string(),
                                lazy_source: Some(source.clone()),
                                current_item: current_item.clone(),
                            };
                            upsert_for_frame(&mut frame.for_stack, entry);
                        }
                    }
                    return Err(EvalError::Signal(ControlFlow::Yield(v)));
                }
                Err(e) => return Err(e),
            }
        }
        // Item finished; the next loop turn pulls a fresh one.
        body_index = 0;
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

    // This while is keyed by its AST byte-offset (stable across resumes), so a
    // nested inner while's resume entry doesn't collide with this one's — the
    // `while_resume` stack can hold both.
    let node_key = node.range.start().to_u32();
    // A resume entry for THIS while means we suspended on a yield at that body
    // statement; re-enter there and skip the condition check for that pass.
    let clear_resume = move |state: &mut InterpreterState| {
        if let Some(&id) = state.active_generator_stack.last() {
            if let Some(frame) = state.generators.get_mut(&id) {
                frame.while_resume.retain(|(k, _)| *k != node_key);
            }
        }
    };
    let (mut body_index, mut resuming) = {
        let saved = state
            .active_generator_stack
            .last()
            .copied()
            .and_then(|id| state.generators.get(&id))
            .and_then(|frame| {
                frame.while_resume.iter().rev().find(|(k, _)| *k == node_key).map(|(_, i)| *i)
            });
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
            // An `if` steps through `run_if_suspendable` so a yield inside its
            // branch (even after side-effecting statements) resumes exactly at
            // the yield via `if_stack`, rather than re-running the branch. A
            // nested `for` steps through `run_for_suspendable` (its `for_stack`
            // is disjoint from this loop's `while_resume`), so
            // `while True: for j in …: yield` suspends correctly.
            let stmt_yields = super::definitions::contains_yield_stmts(std::slice::from_ref(stmt));
            let (result, nested) = match stmt {
                Stmt::If(if_node) => {
                    (run_if_suspendable(state, if_node, tools).await.map(|()| Value::None), true)
                }
                Stmt::For(f) if stmt_yields && for_delegatable(f) => {
                    let target = for_name_target(f).unwrap_or_default().to_string();
                    // Boxed: `run_while` ↔ `run_for` is a mutual async recursion.
                    (
                        Box::pin(run_for_suspendable(state, f, &target, tools))
                            .await
                            .map(|()| Value::None),
                        true,
                    )
                }
                // A nested `while` keys its own `while_resume` entry by AST
                // offset, so `while a: while b: yield` keeps both live.
                Stmt::While(w) if stmt_yields && while_delegatable(w) => {
                    // Boxed: direct async self-recursion.
                    (
                        Box::pin(run_while_suspendable(state, w, tools))
                            .await
                            .map(|()| Value::None),
                        true,
                    )
                }
                _ => (eval_stmt(state, stmt, tools).await, false),
            };
            match result {
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
                            // A nested `if`/`for`/`while` stepper already recorded
                            // its own resume; only a direct yield needs
                            // `resume_at_yield` set here.
                            if !nested {
                                frame.resume_at_yield = true;
                            }
                            // Upsert THIS while's resume position (keyed by offset).
                            if let Some(slot) =
                                frame.while_resume.iter_mut().find(|(k, _)| *k == node_key)
                            {
                                slot.1 = body_index;
                            } else {
                                frame.while_resume.push((node_key, body_index));
                            }
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

/// Run a `with` statement inside a generator, stepping its body so a
/// `yield` inside suspends without running `__exit__`. The context
/// managers are entered once on the first pass; on every resume the
/// already-entered managers are restored from the resume record, and
/// `__exit__` runs only when the body truly completes or errors — this is
/// what makes a `@contextmanager` generator able to `yield` from inside a
/// `with`. Mirrors `run_try_suspendable`.
fn run_with_suspendable<'a>(
    state: &'a mut InterpreterState,
    node: &'a rustpython_parser::ast::StmtWith,
    tools: &'a crate::tools::Tools,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EvalError>> + Send + 'a>> {
    Box::pin(run_with_suspendable_inner(state, node, tools))
}

async fn run_with_suspendable_inner(
    state: &mut InterpreterState,
    node: &rustpython_parser::ast::StmtWith,
    tools: &crate::tools::Tools,
) -> Result<(), EvalError> {
    use crate::eval::control_flow::{call_context_method, exit_context_managers};

    // Pop this nesting level's resume record (LIFO). `None` = a fresh entry:
    // enter every context manager in declaration order and bind its target.
    let (start, managers) = match pop_with_resume(state) {
        Some(resume) => (resume.index, resume.managers),
        None => {
            let mut managers = Vec::with_capacity(node.items.len());
            for item in &node.items {
                let cm = crate::eval::eval_expr(state, &item.context_expr, tools).await?;
                let cm = super::resolve_proxy(&cm).await?;
                let entered = call_context_method(state, &cm, "__enter__", &[], tools).await?;
                if let Some(var_expr) = &item.optional_vars {
                    crate::eval::statements::assign_target(state, var_expr, entered, tools).await?;
                }
                managers.push(cm);
            }
            (0, managers)
        }
    };

    match step_with_body(state, &node.body, start, &managers, tools).await {
        TryStep::Yielded(v) => Err(EvalError::Signal(ControlFlow::Yield(v))),
        // Body finished / errored: exit the managers in reverse now (with
        // CPython's `__exit__` suppression rules), not on the intermediate yields.
        TryStep::Completed => exit_context_managers(state, managers, None, tools).await,
        TryStep::Errored(e) => exit_context_managers(state, managers, Some(e), tools).await,
    }
}

/// Step the body of a `with` from `start`; on a `yield`, record the resume
/// position and the entered managers, then return `Yielded`. Mirrors
/// `step_try_phase`, delegating nested compound statements to their own
/// suspend steppers.
async fn step_with_body(
    state: &mut InterpreterState,
    stmts: &[Stmt],
    start: usize,
    managers: &[Value],
    tools: &crate::tools::Tools,
) -> TryStep {
    let mut i = start;
    while i < stmts.len() {
        let stmt_yields = super::definitions::contains_yield_stmts(std::slice::from_ref(&stmts[i]));
        let (result, nested) = match &stmts[i] {
            Stmt::Try(nested_try) => {
                (run_try_suspendable(state, nested_try, tools).await.map(|()| Value::None), true)
            }
            Stmt::If(if_node)
                if stmt_yields
                    && if_branch_suspendable(&if_node.body)
                    && if_branch_suspendable(&if_node.orelse) =>
            {
                (run_if_suspendable(state, if_node, tools).await.map(|()| Value::None), true)
            }
            Stmt::While(w) if stmt_yields && while_delegatable(w) => {
                (run_while_suspendable(state, w, tools).await.map(|()| Value::None), true)
            }
            Stmt::For(f) if stmt_yields && for_delegatable(f) => {
                let target = for_name_target(f).unwrap_or_default().to_string();
                (run_for_suspendable(state, f, &target, tools).await.map(|()| Value::None), true)
            }
            Stmt::With(w) if stmt_yields && if_branch_suspendable(&w.body) => {
                (run_with_suspendable(state, w, tools).await.map(|()| Value::None), true)
            }
            _ => (eval_stmt(state, &stmts[i], tools).await, false),
        };
        match result {
            Ok(_) => {
                i += 1;
                clear_resume_at_yield(state);
            }
            Err(EvalError::Signal(ControlFlow::Yield(v))) => {
                push_with_resume(
                    state,
                    crate::state::WithResume { index: i, managers: managers.to_vec() },
                );
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

fn push_with_resume(state: &mut InterpreterState, resume: crate::state::WithResume) {
    if let Some(&id) = state.active_generator_stack.last() {
        if let Some(frame) = state.generators.get_mut(&id) {
            frame.with_stack.push(resume);
        }
    }
}

fn pop_with_resume(state: &mut InterpreterState) -> Option<crate::state::WithResume> {
    let id = *state.active_generator_stack.last()?;
    state.generators.get_mut(&id)?.with_stack.pop()
}

fn push_if_resume(state: &mut InterpreterState, resume: crate::state::IfResume) {
    if let Some(&id) = state.active_generator_stack.last() {
        if let Some(frame) = state.generators.get_mut(&id) {
            frame.if_stack.push(resume);
        }
    }
}

fn pop_if_resume(state: &mut InterpreterState) -> Option<crate::state::IfResume> {
    let id = *state.active_generator_stack.last()?;
    state.generators.get_mut(&id)?.if_stack.pop()
}

/// Run an `if` inside a generator, stepping the taken branch
/// statement-by-statement so a `yield` — even one after side-effecting
/// statements — suspends at its own position and resumes there without
/// re-running the branch or re-evaluating the condition. Nested `if`/`try`
/// recurse (their resume positions push onto the respective stacks). A yield
/// buried in a nested `for`/`while` is not resumable this way, so the
/// suspendable gate keeps those on the eager fallback.
fn run_if_suspendable<'a>(
    state: &'a mut InterpreterState,
    node: &'a rustpython_parser::ast::StmtIf,
    tools: &'a crate::tools::Tools,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EvalError>> + Send + 'a>> {
    Box::pin(run_if_suspendable_inner(state, node, tools))
}

async fn run_if_suspendable_inner(
    state: &mut InterpreterState,
    node: &rustpython_parser::ast::StmtIf,
    tools: &crate::tools::Tools,
) -> Result<(), EvalError> {
    // Resuming into a recorded branch skips the condition; a fresh entry
    // evaluates it once (CPython semantics) and picks the branch.
    let (in_orelse, start) = match pop_if_resume(state) {
        Some(r) => (r.in_orelse, r.index),
        None => {
            let cond = crate::eval::eval_expr(state, &node.test, tools).await?;
            let took_else = !crate::eval::op::truthy(state, &cond, tools).await?;
            (took_else, 0)
        }
    };
    let branch: Vec<Stmt> = if in_orelse { node.orelse.clone() } else { node.body.clone() };

    let mut i = start;
    while i < branch.len() {
        let stmt_yields =
            super::definitions::contains_yield_stmts(std::slice::from_ref(&branch[i]));
        let (result, nested) = match &branch[i] {
            Stmt::Try(t) => {
                (run_try_suspendable(state, t, tools).await.map(|()| Value::None), true)
            }
            Stmt::If(n) => (run_if_suspendable(state, n, tools).await.map(|()| Value::None), true),
            Stmt::While(w) if stmt_yields && while_delegatable(w) => {
                (run_while_suspendable(state, w, tools).await.map(|()| Value::None), true)
            }
            Stmt::For(f) if stmt_yields && for_delegatable(f) => {
                let target = for_name_target(f).unwrap_or_default().to_string();
                (run_for_suspendable(state, f, &target, tools).await.map(|()| Value::None), true)
            }
            _ => (eval_stmt(state, &branch[i], tools).await, false),
        };
        match result {
            Ok(_) => {
                i += 1;
                clear_resume_at_yield(state);
            }
            Err(EvalError::Signal(ControlFlow::Yield(v))) => {
                push_if_resume(state, crate::state::IfResume { in_orelse, index: i });
                if !nested {
                    set_resume_at_yield(state);
                }
                return Err(EvalError::Signal(ControlFlow::Yield(v)));
            }
            Err(other) => return Err(other),
        }
    }
    Ok(())
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
        let stmt_yields = super::definitions::contains_yield_stmts(std::slice::from_ref(&stmts[i]));
        let (result, nested) = match &stmts[i] {
            Stmt::Try(nested_try) => {
                (run_try_suspendable(state, nested_try, tools).await.map(|()| Value::None), true)
            }
            Stmt::If(if_node)
                if stmt_yields
                    && if_branch_suspendable(&if_node.body)
                    && if_branch_suspendable(&if_node.orelse) =>
            {
                (run_if_suspendable(state, if_node, tools).await.map(|()| Value::None), true)
            }
            // A yield-bearing loop inside the `try` steps through its own loop
            // stepper so `try: while True: yield` / `try: for x in it: yield x`
            // (cleanup generators) suspends at the yield rather than eagerly
            // draining the loop and running `finally` too early. On resume the
            // `try`'s own resume record re-enters this arm, which restores the
            // loop from `while_resume`/`for_stack`.
            Stmt::While(w) if stmt_yields && while_delegatable(w) => {
                (run_while_suspendable(state, w, tools).await.map(|()| Value::None), true)
            }
            Stmt::For(f) if stmt_yields && for_delegatable(f) => {
                let target = for_name_target(f).unwrap_or_default().to_string();
                (run_for_suspendable(state, f, &target, tools).await.map(|()| Value::None), true)
            }
            _ => (eval_stmt(state, &stmts[i], tools).await, false),
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
    let Value::Lazy { items, cursor_id, .. } = receiver else {
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
        // A lazy iterator is its own iterator.
        "__iter__" => Ok(receiver.clone()),
        _ => Err(InterpreterError::AttributeError(format!(
            "'generator' object has no attribute '{method}'"
        ))
        .into()),
    }
}
