// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use indexmap::IndexMap;
use rustpython_parser::ast::{self, Ranged};

use super::{
    builtins::try_builtin,
    definitions::{
        VariableCheckpoint, apply_function_scope, apply_lambda_scope, contains_yield_stmts,
        writeback_nonlocal_cell,
    },
    helpers::{bytes_fromhex, dict_fromkeys},
    method_dispatch::{CallArgs, dispatch_method},
    params::{bind_params_named, execute_body},
};
use crate::{
    error::{ControlFlow, EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    tools::Tools,
    value::{FunctionDef, LambdaDef, Value},
};

/// Grow the host stack on demand around a Python-call future.
///
/// The interpreter is a recursive-descent async evaluator: one Python
/// frame descends through a chain of ~8 nested async-fn poll frames, so
/// recursion depth is bounded by the host thread's stack, not by
/// `config.max_recursion_depth`. Without this a depth of a few dozen
/// (debug) / few hundred (release) overflows the stack and aborts the
/// process — a sandbox DoS. `stacker` (the crate rustc uses for the
/// same reason) checks the remaining stack on each poll and switches to
/// a fresh segment when it runs low, so recursion runs up to the
/// configured depth limit (which then raises `RecursionError`)
/// regardless of the caller's base stack size.
const STACK_RED_ZONE: usize = 1024 * 1024;
const STACK_GROW_SIZE: usize = 32 * 1024 * 1024;

/// Wrap a future so each poll grows the host stack when it runs low.
/// The inner future is `Box::pin`'d (moving that Python frame's future
/// state onto the heap, which also shrinks the per-frame poll stack),
/// then polled under `stacker::maybe_grow` via `poll_fn` — no `unsafe`
/// pin projection needed.
pub(crate) fn grow_stack<'a, T: 'a>(
    inner: impl std::future::Future<Output = T> + 'a,
) -> impl std::future::Future<Output = T> + 'a {
    let mut boxed = Box::pin(inner);
    std::future::poll_fn(move |cx| {
        stacker::maybe_grow(STACK_RED_ZONE, STACK_GROW_SIZE, || boxed.as_mut().poll(cx))
    })
}

/// Call a user-defined function. The heavy recursion runs behind
/// [`grow_stack`] so deep Python recursion doesn't overflow the host
/// stack (see its docs).
pub(crate) async fn call_user_function(
    state: &mut InterpreterState,
    func_def: &FunctionDef,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> EvalResult {
    // Calling an `async def` does not run the body — it captures the call into a
    // coroutine that is driven later by `await` / `asyncio.run` (which call the
    // inner path directly, bypassing this check). An `async def` that also
    // `yield`s is an async *generator*, not a coroutine: it flows through the
    // normal generator machinery (and `async for` drives it like a generator).
    if func_def.is_async && !func_def.is_generator {
        return Ok(Value::Coroutine(Box::new(crate::value::CoroutineValue {
            func: std::sync::Arc::new(func_def.clone()),
            args: args.to_vec(),
            kwargs: kwargs.clone(),
            awaited: false,
        })));
    }
    grow_stack(call_user_function_inner(state, func_def, args, kwargs, tools)).await
}

/// Run a coroutine's body to completion (the sequential-await drive), returning
/// its `return` value. Bypasses the `is_async` short-circuit in
/// [`call_user_function`] by calling the inner executor directly.
pub(crate) async fn drive_coroutine(
    state: &mut InterpreterState,
    coro: &crate::value::CoroutineValue,
    tools: &Tools,
) -> EvalResult {
    grow_stack(call_user_function_inner(state, &coro.func, &coro.args, &coro.kwargs, tools)).await
}

async fn call_user_function_inner(
    state: &mut InterpreterState,
    func_def: &FunctionDef,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> EvalResult {
    // Enforce frame-depth bound before doing any work — guards against
    // `def f(): f()` exhausting memory indirectly. Paired with an
    // unconditional exit_call below so the counter never leaks on error
    // paths.
    state.enter_call().map_err(EvalError::Interpreter)?;

    // Trivial-frame fast path (G5). Skip the whole frame-setup
    // sequence for parameter-less, closure-free, side-effect-free
    // functions: empty body, `def f(): pass`, `def f(): return 1`, and
    // the no-op helpers LLM-emitted code is full of. The body just
    // runs against the parent scope; there are no parameters to bind,
    // no closure to overlay, no nonlocal cells to manage, no names to
    // checkpoint, no generator yields to collect.
    //
    // Conditions: zero positional/vararg/kwonly/kwargs, empty closure
    // (module-level closures defer to live globals via LEGB anyway, so
    // they're handled correctly without the overlay), no `nonlocal` /
    // `global` declarations, no body-level name bindings, not a
    // generator. Decorators / Partial / BoundMethod call paths reach
    // this entry point via the regular dispatch — they pre-fill args
    // before they get here, so a target with zero declared params
    // still qualifies if the call site supplies no extras.
    if args.is_empty()
        && kwargs.is_empty()
        && func_def.params.args.is_empty()
        && func_def.params.vararg.is_none()
        && func_def.params.kwonlyargs.is_empty()
        && func_def.params.kwarg.is_none()
        && func_def.nonlocal_names.is_empty()
        && func_def.assigned_names.is_empty()
        && func_def.global_names.is_empty()
        && !func_def.is_generator
        // A nested def with a non-empty closure needs its closure overlay
        // applied — the fast path runs the body against the parent scope, so
        // an escaped closure (`def outer(): x=1; def inner(): return x`) would
        // see no `x`. Module-level defs resolve free names to the live module
        // globals via LEGB, so their closure needs no overlay.
        && (func_def.closure.is_empty() || func_def.is_module_level)
    {
        let func_name = func_def.body_cache_key().to_string();
        let body = state.function_bodies.get(&func_name).cloned();

        // Trivially-sync body inline (G4-lite): pattern-match the body
        // for cases that need no async dispatch — `pass`, `return`,
        // `return <constant>`. Saves the `Box::pin(async move {...})`
        // allocation that `execute_body` would otherwise wrap around the
        // body walk. For `def f(): pass` and `def f(): return None` this
        // is the entire call body; the call becomes a single function
        // entry / exit with no future boxing at all.
        if let Some(body_stmts) = body.as_ref() {
            if let Some(direct) = try_eval_trivial_body(body_stmts) {
                state.exit_call();
                return Ok(direct);
            }
        }

        state.body_source_stack.push(func_def.source.clone());
        // A trivial-frame body can still create a nested `def`/`lambda`
        // (`return lambda x: x`); it needs the qualname prefix so its
        // `__qualname__` dots correctly (`f.<locals>.<lambda>`).
        state.qualname_stack.push(format!("{}.<locals>", func_def.display_qualname()));
        // Even on the trivial-frame path the body may create a nested closure
        // that captures a local (`return [lambda: i for i in range(n)]`); it
        // needs a `frame_cell_owners` scope so the capture cell registers on
        // this frame (and the loop/comprehension writes through it).
        state.frame_cell_owners.push(rustc_hash::FxHashMap::default());
        let outcome = if let Some(body_stmts) = body {
            match execute_body(state, body_stmts.as_slice(), tools).await {
                Ok(v) => Ok(v),
                Err(EvalError::Signal(ControlFlow::Return(v))) => Ok(*v),
                Err(e) => Err(e),
            }
        } else {
            Ok(Value::None)
        };
        state.frame_cell_owners.pop();
        state.body_source_stack.pop();
        state.qualname_stack.pop();
        state.exit_call();
        return outcome;
    }

    // Push a frame cell-owners scope. Nested defs encountered in this
    // body will register their nonlocal cell ids here so `set_variable`
    // writes them through. Popped unconditionally on every exit path.
    state.frame_cell_owners.push(rustc_hash::FxHashMap::default());

    // Build local scope from parameters
    let bind_outcome = bind_params_named(
        &func_def.params,
        func_def.display_qualname(),
        args,
        kwargs,
        state,
        tools,
    )
    .await;
    let local_scope = match bind_outcome {
        Ok(s) => s,
        Err(e) => {
            state.frame_cell_owners.pop();
            state.exit_call();
            return Err(e);
        }
    };

    // Snapshot only the names this frame can touch — parameters,
    // closure-captured names, and statically-collected `assigned_names`
    // from the body walker. Excludes names declared `global` (those
    // persist to the module scope by design) and `nonlocal` (those
    // bind to the same variable as the enclosing scope; mutations
    // must propagate up, so the frame must NOT snapshot them).
    //
    // Previously this was an unconditional `state.variables.clone()`
    // of the full map, which dominated per-frame cost — every
    // recursive call cloned an N-entry HashMap whose state survived
    // across `execute_body(...).await`. The checkpoint records only
    // O(params + closure + assigned) entries, sized by the function
    // shape, not the global variable count.
    // Closure keys participate in the checkpoint only for nested
    // defs whose def-time snapshot DIFFERS from the live state at
    // call time:
    //
    // - Module-level defs always skip (the apply_function_scope LEGB rule reads live module
    //   globals; snapshotting them would clone-then-restore and silently lose in-place mutations).
    //
    // - Nested defs whose closure value byte-equals the live state value also skip. This is the
    //   common in-stack mutation case: outer's `items = []` followed by `inner()` where inner
    //   mutates items — the def-time closure captured the same logical value the live state holds,
    //   so the snapshot would be a meaningless deep-clone-then-restore that discards the body's
    //   mutation.
    //
    // - Nested defs whose closure DIFFERS from live state still snapshot (and the overlay below
    //   winning takes precedence) — that's the decorator pattern where each wrapper's captured `fn`
    //   must override the surrounding wrapper's binding.
    let closure_touched = func_def.closure.iter().filter(|(name, value)| {
        if func_def.is_module_level {
            return false;
        }
        !state.variables.get(*name).is_some_and(|live| live == *value)
    });
    let touched: Vec<String> = func_def
        .params
        .args
        .iter()
        .map(|p| p.name.clone())
        .chain(func_def.params.vararg.iter().cloned())
        .chain(func_def.params.kwonlyargs.iter().map(|p| p.name.clone()))
        .chain(func_def.params.kwarg.iter().cloned())
        .chain(closure_touched.map(|(name, _)| name.clone()))
        .chain(func_def.assigned_names.iter().cloned())
        .filter(|n| !func_def.global_names.contains(n) && !func_def.nonlocal_names.contains(n))
        .collect();
    let checkpoint = VariableCheckpoint::capture(state, &touched);

    // Apply closure + nonlocal cell + local-scope bindings as a sync
    // helper call. The helper's stack frame is released before any
    // await below, so its locals don't bloat the recursive call's
    // future-state. On error the checkpoint reverses any partial
    // bindings before exiting the frame.
    if let Err(e) = apply_function_scope(state, func_def, &local_scope) {
        checkpoint.restore(state);
        state.frame_cell_owners.pop();
        state.exit_call();
        return Err(e);
    }

    // Retrieve the function body AST from the state's function_bodies
    // map. The `.cloned()` clones an `Arc<Vec<Stmt>>` — pointer + atomic
    // increment, no AST copy. This is the load-bearing cost in the
    // recursive-frame path: the cloned body survives across
    // `execute_body(...).await` below as part of the async fn's
    // captured-locals set, so keeping it pointer-sized is what stops
    // per-frame stack pressure scaling with function body size.
    let func_name = func_def.body_cache_key().to_string();
    let body = state.function_bodies.get(&func_name).cloned();

    // Push the function's source onto the body-source stack so
    // `eval_stmt` stamps inner errors with line numbers from the
    // function's defining source, not from the calling execute()'s
    // source. Popped unconditionally below after the body runs.
    state.body_source_stack.push(func_def.source.clone());
    // Nested `def`/`lambda`/`class` in this body take `<this qualname>.<locals>`
    // as their `__qualname__` prefix (CPython's `<locals>` marker). Popped on
    // every exit path below, mirroring `body_source_stack`.
    state.qualname_stack.push(format!("{}.<locals>", func_def.display_qualname()));

    // If the body contains `yield` / `yield from`, return a suspended
    // generator frame when the body shape is supported. Bodies with
    // while-loop suspension still fall back to an eager Lazy buffer;
    // true while-state resume is tracked by
    // gap-generator-while-loop-suspend-state.
    // Generator flag was set at function-def time; for state imports
    // that predate the cached field (default = false) the body may
    // still carry a yield — fall back to the walk in that case.
    let is_generator =
        func_def.is_generator || body.as_ref().is_some_and(|stmts| contains_yield_stmts(stmts));

    let exec_result = if let Some(body_stmts) = body {
        if is_generator {
            // Prefer true suspend frames. A `while` loop is suspendable only in
            // the top-level, direct-yield shape; other while shapes still fall
            // back to the eager Lazy buffer (gap-generator-while-loop-suspend-state).
            let use_suspend = super::generators::generator_suspendable(body_stmts.as_slice());
            if use_suspend {
                let mut locals = rustc_hash::FxHashMap::default();
                for name in &touched {
                    if let Some(v) = state.variables.get(name) {
                        locals.insert(name.clone(), v.clone());
                    }
                }
                let generator = super::generators::create_generator(
                    state,
                    func_def,
                    body_stmts,
                    locals,
                    touched.clone(),
                );
                writeback_nonlocal_cell(state, func_def);
                checkpoint.restore(state);
                state.frame_cell_owners.pop();
                state.exit_call();
                state.body_source_stack.pop();
                state.qualname_stack.pop();
                return Ok(generator);
            }
            // Eager buffer fallback (while-based generators).
            state.yield_stack.push(Vec::new());
            let body_result = execute_body(state, body_stmts.as_slice(), tools).await;
            let collected = state.yield_stack.pop().unwrap_or_default();
            match body_result {
                Ok(_) | Err(EvalError::Signal(ControlFlow::Return(_))) => {
                    let cursor_id = state.next_cursor_id;
                    state.next_cursor_id = state.next_cursor_id.wrapping_add(1);
                    state.lazy_cursors.insert(cursor_id, 0);
                    Ok(Value::Lazy {
                        items: collected,
                        cursor_id,
                        kind: crate::value::LazyKind::Generator,
                    })
                }
                Err(EvalError::Signal(ControlFlow::Yield(_))) => {
                    // Should not happen with yield_stack path.
                    Err(InterpreterError::Runtime("internal yield without stack".into()).into())
                }
                Err(e) => Err(e),
            }
        } else {
            execute_body(state, body_stmts.as_slice(), tools).await
        }
    } else {
        Ok(Value::None)
    };

    // Write back the post-body values of `nonlocal`-declared names to
    // the shared cell so the next call observes the mutation. Scoped
    // tight to keep the writeback HashMap off the future's
    // surviving-locals set.
    writeback_nonlocal_cell(state, func_def);

    // Restore the touched names and exit the frame regardless of
    // outcome. Globally-declared names are NOT in the checkpoint, so
    // their assignments persist to the module scope (CPython
    // semantics).
    checkpoint.restore(state);
    // Pop the frame cell-owners scope pushed on entry.
    state.frame_cell_owners.pop();
    state.exit_call();
    // Pop the function's source. Matched 1:1 with the push above,
    // including on every error/early-return path through this fn.
    state.body_source_stack.pop();
    state.qualname_stack.pop();

    // Propagate the result, handling Return signals
    match exec_result {
        Ok(val) => Ok(val),
        Err(EvalError::Signal(ControlFlow::Return(val))) => Ok(*val),
        Err(e) => Err(e),
    }
}

/// Scan a statement list for any `yield` or `yield from` expression.
/// Used by `call_user_function` to decide whether to switch to
/// generator mode. Walks every statement and recurses into nested
/// blocks (if / for / while / try / with / match), but does NOT
/// recurse into nested function or class bodies — a yield inside an
/// inner `def` belongs to THAT inner generator, not the outer one.
/// Inline-evaluate a function body that's trivially synchronous — a
/// single `Pass`, a single bare `return`, or a single `return
/// <constant>`. Returns `Some(value)` when matched, `None` otherwise
/// so the caller falls back to the regular `execute_body` path.
///
/// Used by `call_user_function`'s trivial-frame fast path to skip the
/// `Box::pin(async move {...})` that `execute_body` would otherwise
/// allocate. The single-stmt bodies handled here cover the
/// majority of LLM-emitted helper shapes (`def get_default(): return
/// None`, no-op wrappers, sentinel returns, etc.) plus the
/// `frames/empty_function_call_10k` bench's `def f(): pass`.
fn try_eval_trivial_body(body: &[ast::Stmt]) -> Option<Value> {
    if body.len() != 1 {
        return None;
    }
    match &body[0] {
        ast::Stmt::Pass(_) => Some(Value::None),
        ast::Stmt::Return(node) => {
            node.value.as_ref().map_or(Some(Value::None), |expr| match expr.as_ref() {
                ast::Expr::Constant(c) => Some(crate::eval::literals::eval_constant(&c.value)),
                _ => None,
            })
        }
        _ => None,
    }
}

/// Call a lambda expression.
pub(crate) async fn call_lambda(
    state: &mut InterpreterState,
    lambda_def: &LambdaDef,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> EvalResult {
    grow_stack(call_lambda_inner(state, lambda_def, args, kwargs, tools)).await
}

async fn call_lambda_inner(
    state: &mut InterpreterState,
    lambda_def: &LambdaDef,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> EvalResult {
    // Frame-depth bound — same reasoning as `call_user_function`.
    state.enter_call().map_err(EvalError::Interpreter)?;

    let lambda_qualname =
        if lambda_def.qualname.is_empty() { "<lambda>" } else { lambda_def.qualname.as_str() };
    let bind_outcome =
        bind_params_named(&lambda_def.params, lambda_qualname, args, kwargs, state, tools).await;
    let local_scope = match bind_outcome {
        Ok(s) => s,
        Err(e) => {
            state.exit_call();
            return Err(e);
        }
    };

    // Same LEGB rule as `call_user_function`: skip closure keys for
    // module-level lambdas, and additionally skip keys whose def-
    // time snapshot byte-equals the live state (the in-stack
    // mutation case where the snapshot would just clone-then-
    // restore over the body's in-place mutations).
    let closure_touched = lambda_def.closure.iter().filter(|(name, value)| {
        if lambda_def.is_module_level {
            return false;
        }
        !state.variables.get(*name).is_some_and(|live| live == *value)
    });
    let touched: Vec<String> = lambda_def
        .params
        .args
        .iter()
        .map(|p| p.name.clone())
        .chain(lambda_def.params.vararg.iter().cloned())
        .chain(lambda_def.params.kwonlyargs.iter().map(|p| p.name.clone()))
        .chain(lambda_def.params.kwarg.iter().cloned())
        .chain(closure_touched.map(|(name, _)| name.clone()))
        .chain(lambda_def.assigned_names.iter().cloned())
        .collect();
    let checkpoint = VariableCheckpoint::capture(state, &touched);

    if let Err(e) = apply_lambda_scope(state, lambda_def, &local_scope) {
        checkpoint.restore(state);
        state.exit_call();
        return Err(e);
    }

    // Retrieve the lambda body AST
    let body = state.lambda_bodies.get(&lambda_def.lambda_id).cloned();

    state.body_source_stack.push(lambda_def.source.clone());
    // A lambda nested in this body (`lambda: lambda: 0`) dots onto
    // `<this qualname>.<locals>`.
    state.qualname_stack.push(format!("{lambda_qualname}.<locals>"));

    let result = if let Some(body_expr) = body {
        // Lambda bodies are expressions, so they never cross an `eval_stmt`
        // boundary — the only place line-stamping happens. Without this, an
        // error raised in the body bubbles up unstamped and is stamped with
        // the *call site's* line. Stamp the body's own line (relative to the
        // lambda's source, the same `body_source_stack` convention `eval_stmt`
        // uses for persisted function bodies) so the agent loop rewrites the
        // lambda, not the call site. `stamp_line` is first-wins, so a deeper
        // error inside the body keeps its own, more-specific line.
        let body_line =
            crate::eval::line_of(&lambda_def.source, body_expr.range().start().to_usize());
        crate::eval::eval_expr(state, &body_expr, tools)
            .await
            .map_err(|e| crate::eval::stamp_line(e, body_line))
    } else {
        Ok(Value::None)
    };

    state.body_source_stack.pop();
    state.qualname_stack.pop();
    checkpoint.restore(state);
    state.exit_call();
    result
}

/// Call a Value that should be a callable. Handles the full surface
/// of "things the user can stash into a variable or pass through a
/// higher-order builtin": user functions/lambdas, bound and unbound
/// methods on builtins, module functions (`json.dumps`), class
/// static/class methods, the type sentinels for builtin functions
/// (`__builtin__len`), and tool sentinels.
///
/// kwargs are dropped on the sentinel paths (tool/builtin/module/
/// class method via indirection) because the higher-order callers
/// (`map`, `filter`, `apply_key_fn`, `eval_call` variable-lookup)
/// don't carry kwargs at this surface anyway. `Value::Function` /
/// `Value::Lambda` already accept kwargs via their direct
/// `eval_call` paths — those bypass this helper.
pub(crate) async fn call_value_as_function(
    state: &mut InterpreterState,
    func: &Value,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> EvalResult {
    match func {
        Value::Function(func_def) => call_user_function(state, func_def, args, kwargs, tools).await,
        Value::Lambda(lambda_def) => call_lambda(state, lambda_def, args, kwargs, tools).await,
        // A class value obtained from an expression (`type(n, b, ns)()`,
        // `classes[0]()`, `make_class()()`) instantiates, exactly as the
        // bare-name call path does.
        Value::Class(class_name) => {
            crate::eval::classes::instantiate(state, class_name, args, kwargs, tools).await
        }
        // A bound builtin method (`d.get`, `s.upper`, ...) passed as
        // a first-class callable. Two receiver shapes:
        //
        // - Snapshot: the receiver was captured by value (a temporary or non-place expression).
        //   Dispatch on the clone; the mem_delta is discarded because mutations have nowhere to
        //   propagate to. Matches CPython for non-place receivers.
        //
        // - Place: the receiver was a navigable place at capture time. Navigate live state to reach
        //   the slot and dispatch in-place. Mutations through the bound method (xs.append stashed
        //   as `push`) propagate back to the original variable. Memory delta applied to the budget
        //   after the borrow ends.
        Value::BoundMethod { receiver, method } => {
            match receiver {
                crate::value::BoundMethodReceiver::Snapshot(value) => {
                    if matches!(
                        **value,
                        Value::Lazy { .. } | Value::Generator { .. } | Value::BuiltinIter { .. }
                    ) && super::generators::is_generator_method(method)
                    {
                        return super::generators::dispatch_generator_method(
                            state, value, method, args, kwargs, tools,
                        )
                        .await;
                    }
                    // A bound user-instance method (`m = p.go; m()`) runs the
                    // async class method against the captured instance. Its
                    // `fields` are shared via Arc, so self-mutations propagate
                    // back to the original object. dispatch_method is sync and
                    // has no view of the class registry, so it cannot run this.
                    if let Value::Instance(inst) = &**value {
                        if let Some((_, def)) = crate::eval::classes::lookup_method_in_mro(
                            state,
                            &inst.class_name,
                            method,
                        ) {
                            let call = crate::eval::functions::CallArgs {
                                positional: args,
                                keyword: kwargs,
                            };
                            let (returned, _self) = crate::eval::classes::call_method(
                                state,
                                &def,
                                (**value).clone(),
                                call,
                                tools,
                            )
                            .await?;
                            return Ok(returned);
                        }
                    }
                    // A captured `__iter__` bound method (`it = xs.__iter__; it()`)
                    // builds a fresh iterator, needing async state.
                    if method == "__iter__"
                        && args.is_empty()
                        && crate::types::builtin_dunder_present(value, "__iter__")
                    {
                        return super::builtins::make_iterator(state, value, tools).await;
                    }
                    // A captured classmethod (`f = {}.fromkeys; f(...)`) routes
                    // through the type-form dispatch (ignoring the receiver).
                    if let Some((type_name, m)) = crate::types::instance_classmethod(value, method)
                    {
                        let unbound = Value::BuiltinTypeMethod {
                            type_name: type_name.to_string(),
                            method: m.to_string(),
                        };
                        return Box::pin(call_value_as_function(
                            state, &unbound, args, kwargs, tools,
                        ))
                        .await;
                    }
                    let mut recv = (**value).clone();
                    Ok(dispatch_method(&mut recv, method, args, kwargs)?.value)
                }
                crate::value::BoundMethodReceiver::Place { root, steps } => {
                    use crate::{
                        eval::place::{PlaceStep, apply_mem_delta, with_navigate_mut},
                        value::BoundMethodStep,
                    };

                    let pl_steps: Vec<PlaceStep> = steps
                        .iter()
                        .map(|s| match s {
                            BoundMethodStep::Index(v) => PlaceStep::Index(v.clone()),
                            BoundMethodStep::Attr(n) => PlaceStep::Attr(n.clone()),
                        })
                        .collect();

                    // Generator methods need `&mut state` for the cursor map —
                    // classify under the place borrow, then dispatch after.
                    let gen_recv = {
                        let root_slot = state.variables.get_mut(root).ok_or_else(|| {
                            EvalError::from(InterpreterError::name_not_defined(root))
                        })?;
                        with_navigate_mut(root_slot, &pl_steps, |target| {
                            if matches!(
                                target,
                                Value::Lazy { .. }
                                    | Value::Generator { .. }
                                    | Value::BuiltinIter { .. }
                            ) && super::generators::is_generator_method(method)
                            {
                                Ok::<Option<Value>, EvalError>(Some(target.clone()))
                            } else {
                                Ok(None)
                            }
                        })??
                    };
                    if let Some(recv) = gen_recv {
                        return super::generators::dispatch_generator_method(
                            state, &recv, method, args, kwargs, tools,
                        )
                        .await;
                    }
                    let outcome = {
                        let root_slot = state.variables.get_mut(root).ok_or_else(|| {
                            EvalError::from(InterpreterError::name_not_defined(root))
                        })?;
                        with_navigate_mut(root_slot, &pl_steps, |target| {
                            dispatch_method(target, method, args, kwargs)
                        })??
                    };
                    apply_mem_delta(state, outcome.mem_delta)?;
                    Ok(outcome.value)
                }
            }
        }
        // Unbound method descriptor — `str.upper`, `list.append`.
        // First positional arg becomes the receiver; rest are the
        // method's own arguments. CPython:
        // `str.upper("abc") == "abc".upper() == "ABC"`.
        Value::BuiltinTypeMethod { type_name, method } => {
            // Classmethod-style entries that take no receiver. CPython:
            // `dict.fromkeys(iterable, value=None)`,
            // `bytes.fromhex(hex_str)`. The args list here is the
            // call args directly (no instance), so route around
            // dispatch_method which assumes args[0] is the receiver.
            if type_name == "dict" && method == "fromkeys" {
                return dict_fromkeys(state, args, tools).await;
            }
            if (type_name == "bytes" || type_name == "bytearray") && method == "fromhex" {
                let parsed = bytes_fromhex(args)?;
                // `bytearray.fromhex` returns a bytearray; `bytes.fromhex` bytes.
                return Ok(match (type_name.as_str(), parsed) {
                    ("bytearray", Value::Bytes(b)) => {
                        Value::ByteArray(crate::value::shared_bytes(b))
                    }
                    (_, other) => other,
                });
            }
            if type_name == "int" && method == "from_bytes" {
                return crate::eval::functions::helpers::int_from_bytes(args, kwargs);
            }
            // bool is an int subclass, so `bool.from_bytes` reuses int's but
            // truthifies the result.
            if type_name == "bool" && method == "from_bytes" {
                let n = crate::eval::functions::helpers::int_from_bytes(args, kwargs)?;
                return Ok(Value::Bool(n.is_truthy()));
            }
            if type_name == "str" && method == "maketrans" {
                return crate::eval::functions::helpers::str_maketrans(args);
            }
            // `bytes.maketrans(from, to)` — a 256-byte translation table.
            if (type_name == "bytes" || type_name == "bytearray") && method == "maketrans" {
                return bytes_maketrans(args);
            }
            if type_name == "float" && method == "fromhex" {
                return crate::eval::functions::helpers::float_fromhex(args);
            }
            // `object.__setattr__(inst, name, value)` etc. — the default
            // implementations, called directly to bypass a custom override
            // (e.g. from within a class's own `__setattr__`).
            if type_name == "object" {
                return object_default_method(state, method, args, tools).await;
            }
            let Some((recv_arg, rest)) = args.split_first() else {
                return Err(InterpreterError::TypeError(format!(
                    "unbound method {type_name}.{method}() needs a {type_name} as first argument"
                ))
                .into());
            };
            let mut recv = recv_arg.clone();
            Ok(dispatch_method(&mut recv, method, rest, kwargs)?.value)
        }
        // `from json import dumps` stored as a variable, then passed
        // through map/filter/key=. The eval_call name-lookup branch
        // already calls module dispatch directly for the direct-call
        // form; this arm covers the indirection form.
        Value::ModuleFunction { module, name } => {
            crate::eval::modules::call_function(state, module, name, args, kwargs, tools).await
        }
        // Bare builtin function name passed as a value — `try_builtin`
        // is the canonical dispatch; route there with empty kwargs.
        Value::BuiltinName(builtin_name) => {
            // Box::pin breaks the async recursion: try_builtin → min/max →
            // apply_key_fn → call_value_as_function → try_builtin. The
            // future graph is otherwise infinitely-sized at compile time.
            Box::pin(try_builtin(state, builtin_name, args, kwargs, tools)).await?.ok_or_else(
                || InterpreterError::TypeError(format!("'{builtin_name}' is not callable")).into(),
            )
        }
        // Bare tool name passed through indirection. resolve_and_dispatch
        // is the canonical entry; we use the same ToolCallDescriptor.
        Value::ToolName(tool_name) => crate::tools::resolver::resolve_and_dispatch(
            state,
            crate::tools::resolver::ToolCallDescriptor { name: tool_name, args, kwargs },
            tools,
        )
        .await?
        .ok_or_else(|| {
            InterpreterError::TypeError(format!("'{tool_name}' is not callable")).into()
        }),
        // Class method captured as a value (`Cls.classmethod` form).
        // Dispatch via call_method with the class as receiver — CPython
        // classmethod descriptor semantics.
        Value::UnboundClassMethod { class, method } => {
            let Some(def) = crate::eval::classes::lookup_class_method(state, class, method) else {
                return Err(InterpreterError::AttributeError(format!(
                    "type object '{class}' has no classmethod '{method}'"
                ))
                .into());
            };
            let call = CallArgs { positional: args, keyword: kwargs };
            let (returned, _self) = crate::eval::classes::call_method(
                state,
                &def,
                Value::Class(class.clone()),
                call,
                tools,
            )
            .await?;
            Ok(returned)
        }
        // Exception type constructor: `ValueError("msg")` produces a
        // Value::Exception. CPython sets `e.message` to the first arg
        // (Display-formatted) when there's exactly one, joins them
        // with `, ` when there are multiple, and empty otherwise.
        // `e.args` carries the EXACT call arguments (preserving
        // types) so user code that inspects `e.args[0]` sees the
        // original value, not a stringification.
        Value::ExceptionMethod { method, exception } => {
            crate::eval::exceptions::call_exception_method(method, exception, args)
        }
        Value::ExceptionType(type_name) => {
            crate::eval::exceptions::construct_exception_type(type_name, args)
        }
        // `functools.partial` — prepend the bound args and forward to
        // `functools.partial` — prepend the bound positional args and merge the
        // bound keywords, then forward to the bound function. The bound keywords
        // are defaults; a call-site keyword of the same name overrides them
        // (`functools.partial` semantics). Previously `data.keywords` was never
        // read, so `partial(f, mode="x")()` silently lost `mode`.
        Value::Partial(data) => {
            let target = &data.func;
            let mut combined: Vec<Value> = Vec::with_capacity(data.args.len() + args.len());
            combined.extend(data.args.iter().cloned());
            combined.extend_from_slice(args);

            let merged_kwargs = if data.keywords.is_empty() {
                kwargs.clone()
            } else {
                let mut merged = data.keywords.clone();
                for (k, v) in kwargs {
                    merged.insert(k.clone(), v.clone());
                }
                merged
            };
            return Box::pin(call_value_as_function(
                state,
                target,
                &combined,
                &merged_kwargs,
                tools,
            ))
            .await;
        }
        // `operator.itemgetter` / `attrgetter` / `methodcaller` — apply the
        // captured getter to the single argument.
        Value::OperatorGetter(getter) => {
            let [obj] = args else {
                return Err(InterpreterError::TypeError(format!(
                    "{} expected 1 argument, got {}",
                    func.type_name(),
                    args.len()
                ))
                .into());
            };
            return apply_operator_getter(state, getter, obj, tools).await;
        }
        Value::SingleDispatch(sd) => {
            // Dispatch on the type of the first positional argument, walking
            // its MRO to find a registered implementation; fall back to the
            // default. The chosen implementation receives the full argument
            // list unchanged.
            let impl_fn =
                crate::eval::modules::functools::resolve_dispatch_impl(sd, args.first(), state);
            return Box::pin(call_value_as_function(state, &impl_fn, args, kwargs, tools)).await;
        }
        Value::LruCache(data) => {
            // Memoize by positional AND keyword ValueKeys. Keying on positionals
            // only meant `f(1, b=2)` and `f(1, b=3)` collided on the same cache
            // slot — the second call returned the first's result. The keyword
            // half is sorted by name so call-order does not change the key.
            use crate::eval::literals::value_to_key;
            let mut key: Vec<_> =
                args.iter().map(value_to_key).collect::<Result<Vec<_>, _>>().map_err(|_| {
                    InterpreterError::TypeError("lru_cache arguments must be hashable".into())
                })?;
            let mut kw: Vec<(&String, &Value)> = kwargs.iter().collect();
            kw.sort_by(|a, b| a.0.cmp(b.0));
            for (name, value) in kw {
                key.push(crate::value::ValueKey::String(name.as_str().into()));
                key.push(value_to_key(value).map_err(|_| {
                    InterpreterError::TypeError("lru_cache arguments must be hashable".into())
                })?);
            }
            {
                let mut cache = data.cache.lock();
                if let Some(hit) = cache.get(&key) {
                    // LRU: move hit to end
                    let hit = hit.clone();
                    cache.shift_remove(&key);
                    cache.insert(key.clone(), hit.clone());
                    data.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return Ok(hit);
                }
            }
            data.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let result =
                Box::pin(call_value_as_function(state, &data.func, args, kwargs, tools)).await?;
            let mut cache = data.cache.lock();
            if let Some(max) = data.maxsize {
                while cache.len() >= max && max > 0 {
                    cache.shift_remove_index(0);
                }
            }
            if data.maxsize != Some(0) {
                cache.insert(key, result.clone());
            }
            Ok(result)
        }
        // User-class instance with `__call__`: dispatch the slot via
        // `call_method`. Callable instances (factories, partial-
        // application objects, configured strategy patterns) are a
        // common customer-emitted shape.
        Value::Instance(inst) => {
            let class_name = inst.class_name.clone();
            if let Some((_, method)) =
                crate::eval::classes::lookup_method_in_mro(state, &class_name, "__call__")
            {
                let call = CallArgs { positional: args, keyword: kwargs };
                let (returned, _self) =
                    crate::eval::classes::call_method(state, &method, func.clone(), call, tools)
                        .await?;
                return Ok(returned);
            }
            Err(InterpreterError::TypeError(format!("'{class_name}' object is not callable"))
                .into())
        }
        Value::None => {
            // filter(None, ...) uses truthiness — shouldn't reach here
            Err(InterpreterError::TypeError("'NoneType' object is not callable".into()).into())
        }
        _ => Err(InterpreterError::TypeError(format!(
            "'{}' object is not callable",
            func.type_name()
        ))
        .into()),
    }
}

/// `bytes.maketrans(from, to)` — build the 256-byte identity table then map
/// each byte in `from` to the byte at the same index in `to`.
fn bytes_maketrans(args: &[Value]) -> EvalResult {
    let bytes_of = |v: Option<&Value>| -> Option<Vec<u8>> {
        match v {
            Some(Value::Bytes(b)) => Some(b.clone()),
            Some(Value::ByteArray(b)) => Some(b.lock().clone()),
            _ => None,
        }
    };
    let (Some(from), Some(to)) = (bytes_of(args.first()), bytes_of(args.get(1))) else {
        return Err(InterpreterError::TypeError(
            "maketrans() requires two bytes-like objects".into(),
        )
        .into());
    };
    if from.len() != to.len() {
        return Err(InterpreterError::ValueError(
            "maketrans arguments must have same length".into(),
        )
        .into());
    }
    let mut table: Vec<u8> = (0..=255).collect();
    for (&f, &t) in from.iter().zip(&to) {
        table[f as usize] = t;
    }
    Ok(Value::Bytes(table))
}

/// The default `object.__setattr__` / `__delattr__` / `__getattribute__` /
/// `__init__`, called directly (`object.__setattr__(self, name, value)`) to
/// bypass a class's own override. Operates straight on the instance's fields.
async fn object_default_method(
    state: &mut InterpreterState,
    method: &str,
    args: &[Value],
    tools: &Tools,
) -> EvalResult {
    let inst = match args.first() {
        Some(Value::Instance(inst)) => inst,
        _ => {
            return Err(InterpreterError::TypeError(format!(
                "descriptor '{method}' requires a 'object' instance"
            ))
            .into());
        }
    };
    match method {
        "__setattr__" => {
            let (Some(Value::String(name)), Some(value)) = (args.get(1), args.get(2)) else {
                return Err(InterpreterError::TypeError(
                    "object.__setattr__ requires a name and a value".into(),
                )
                .into());
            };
            inst.fields.lock().insert(name.to_string(), value.clone());
            Ok(Value::None)
        }
        "__delattr__" => {
            let Some(Value::String(name)) = args.get(1) else {
                return Err(InterpreterError::TypeError(
                    "object.__delattr__ requires a name".into(),
                )
                .into());
            };
            if inst.fields.lock().remove(name.as_str()).is_none() {
                return Err(InterpreterError::AttributeError(name.to_string()).into());
            }
            Ok(Value::None)
        }
        "__getattribute__" => {
            let Some(Value::String(name)) = args.get(1) else {
                return Err(InterpreterError::TypeError(
                    "object.__getattribute__ requires a name".into(),
                )
                .into());
            };
            // The default lookup protocol (descriptors, instance dict, class
            // attrs, methods). Bypasses any user `__getattribute__` override —
            // this is exactly what `super().__getattribute__(name)` should
            // reach — so a subclass override that delegates here for the
            // uninteresting names resolves methods and properties, not just
            // instance fields.
            crate::eval::names::getattr_normal_lookup(
                state,
                Value::Instance(inst.clone()),
                name.as_str(),
                tools,
                None,
            )
            .await
        }
        // `object.__init__` / `__new__` accept anything and do nothing useful here.
        "__init__" => Ok(Value::None),
        _ => Err(InterpreterError::AttributeError(format!(
            "type object 'object' has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Apply an `operator.itemgetter`/`attrgetter`/`methodcaller` to its argument.
async fn apply_operator_getter(
    state: &mut crate::state::InterpreterState,
    getter: &crate::value::OperatorGetter,
    obj: &Value,
    tools: &Tools,
) -> EvalResult {
    use crate::value::OperatorGetter;
    match getter {
        OperatorGetter::ItemGetter(items) => {
            let mut results = Vec::with_capacity(items.len());
            for item in items {
                results.push(crate::eval::op::getitem(state, obj, item, tools).await?);
            }
            Ok(single_or_tuple(results))
        }
        OperatorGetter::AttrGetter(attrs) => {
            let mut results = Vec::with_capacity(attrs.len());
            for parts in attrs {
                // A dotted path (`attrgetter("a.b")`) resolves left to right.
                let mut current = obj.clone();
                for part in parts {
                    current =
                        crate::eval::names::getattr_on_value(state, current, part, tools, None)
                            .await?;
                }
                results.push(current);
            }
            Ok(single_or_tuple(results))
        }
        OperatorGetter::MethodCaller { name, args, kwargs } => {
            // A user instance's method is looked up in its MRO and invoked with
            // `self`; a builtin receiver goes through the method-dispatch table.
            if let Value::Instance(inst) = obj {
                if let Some((_, method)) =
                    crate::eval::classes::lookup_method_in_mro(state, &inst.class_name, name)
                {
                    let call = CallArgs { positional: args, keyword: kwargs };
                    let (returned, _self) =
                        crate::eval::classes::call_method(state, &method, obj.clone(), call, tools)
                            .await?;
                    return Ok(returned);
                }
            }
            let mut receiver = obj.clone();
            Ok(dispatch_method(&mut receiver, name, args, kwargs)?.value)
        }
    }
}

/// A single result is returned bare; two or more form a tuple (CPython
/// `itemgetter`/`attrgetter` with multiple selectors).
fn single_or_tuple(mut results: Vec<Value>) -> Value {
    if results.len() == 1 { results.pop().unwrap_or(Value::None) } else { Value::Tuple(results) }
}
