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
    params::{bind_params, execute_body},
};
use crate::{
    error::{ControlFlow, EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    tools::Tools,
    value::{FunctionDef, LambdaDef, Value},
};

/// Call a user-defined function.
pub(crate) async fn call_user_function(
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
    {
        let func_name = func_def.name.clone();
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
        let outcome = if let Some(body_stmts) = body {
            match execute_body(state, body_stmts.as_slice(), tools).await {
                Ok(v) => Ok(v),
                Err(EvalError::Signal(ControlFlow::Return(v))) => Ok(*v),
                Err(e) => Err(e),
            }
        } else {
            Ok(Value::None)
        };
        state.body_source_stack.pop();
        state.exit_call();
        return outcome;
    }

    // Push a frame cell-owners scope. Nested defs encountered in this
    // body will register their nonlocal cell ids here so `set_variable`
    // writes them through. Popped unconditionally on every exit path.
    state.frame_cell_owners.push(rustc_hash::FxHashMap::default());

    // Build local scope from parameters
    let bind_outcome = bind_params(&func_def.params, args, kwargs, state, tools).await;
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
    let checkpoint = VariableCheckpoint::capture(state, touched.clone());

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
    let func_name = func_def.name.clone();
    let body = state.function_bodies.get(&func_name).cloned();

    // Push the function's source onto the body-source stack so
    // `eval_stmt` stamps inner errors with line numbers from the
    // function's defining source, not from the calling execute()'s
    // source. Popped unconditionally below after the body runs.
    state.body_source_stack.push(func_def.source.clone());

    // Track C: if the body contains a `yield` or `yield from`, the
    // function is a generator. Run it eagerly to completion, collecting
    // yielded values into a buffer; return the buffer as a list so the
    // caller can iterate it. This is the pragmatic shape that handles
    // `for x in gen()`, `list(gen())`, `sum(gen())`, and `yield from`
    // delegation. `next/send/throw/close` need a real coroutine and
    // are reserved for a follow-up.
    // Generator flag was set at function-def time; for state imports
    // that predate the cached field (default = false) the body may
    // still carry a yield — fall back to the walk in that case.
    let is_generator =
        func_def.is_generator || body.as_ref().is_some_and(|stmts| contains_yield_stmts(stmts));

    let exec_result = if let Some(body_stmts) = body {
        if is_generator {
            // Prefer true suspend frames; fall back to eager Lazy buffer when
            // the body uses `while` (suspend state for while not yet modelled).
            let use_suspend = !super::generators::body_has_while(body_stmts.as_slice());
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
                    Ok(Value::Lazy { items: collected, cursor_id })
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
    // Frame-depth bound — same reasoning as `call_user_function`.
    state.enter_call().map_err(EvalError::Interpreter)?;

    let bind_outcome = bind_params(&lambda_def.params, args, kwargs, state, tools).await;
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
    let checkpoint = VariableCheckpoint::capture(state, touched);

    if let Err(e) = apply_lambda_scope(state, lambda_def, &local_scope) {
        checkpoint.restore(state);
        state.exit_call();
        return Err(e);
    }

    // Retrieve the lambda body AST
    let body = state.lambda_bodies.get(&lambda_def.lambda_id).cloned();

    state.body_source_stack.push(lambda_def.source.clone());

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
    tools: &Tools,
) -> EvalResult {
    match func {
        Value::Function(func_def) => {
            call_user_function(state, func_def, args, &IndexMap::new(), tools).await
        }
        Value::Lambda(lambda_def) => {
            call_lambda(state, lambda_def, args, &IndexMap::new(), tools).await
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
                    let empty_kwargs = IndexMap::new();
                    if matches!(**value, Value::Lazy { .. } | Value::Generator { .. })
                        && super::generators::is_generator_method(method)
                    {
                        return super::generators::dispatch_generator_method(
                            state,
                            value,
                            method,
                            args,
                            &empty_kwargs,
                            tools,
                        )
                        .await;
                    }
                    let mut recv = (**value).clone();
                    Ok(dispatch_method(&mut recv, method, args, &empty_kwargs)?.value)
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

                    let empty_kwargs = IndexMap::new();
                    // Generator methods need `&mut state` for the cursor map —
                    // classify under the place borrow, then dispatch after.
                    let gen_recv = {
                        let root_slot = state.variables.get_mut(root).ok_or_else(|| {
                            EvalError::from(InterpreterError::name_not_defined(root))
                        })?;
                        with_navigate_mut(root_slot, &pl_steps, |target| {
                            if matches!(target, Value::Lazy { .. } | Value::Generator { .. })
                                && super::generators::is_generator_method(method)
                            {
                                Ok::<Option<Value>, EvalError>(Some(target.clone()))
                            } else {
                                Ok(None)
                            }
                        })??
                    };
                    if let Some(recv) = gen_recv {
                        return super::generators::dispatch_generator_method(
                            state,
                            &recv,
                            method,
                            args,
                            &empty_kwargs,
                            tools,
                        )
                        .await;
                    }
                    let outcome = {
                        let root_slot = state.variables.get_mut(root).ok_or_else(|| {
                            EvalError::from(InterpreterError::name_not_defined(root))
                        })?;
                        with_navigate_mut(root_slot, &pl_steps, |target| {
                            dispatch_method(target, method, args, &empty_kwargs)
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
                return bytes_fromhex(args);
            }
            let Some((recv_arg, rest)) = args.split_first() else {
                return Err(InterpreterError::TypeError(format!(
                    "unbound method {type_name}.{method}() needs a {type_name} as first argument"
                ))
                .into());
            };
            let mut recv = recv_arg.clone();
            let empty_kwargs = IndexMap::new();
            Ok(dispatch_method(&mut recv, method, rest, &empty_kwargs)?.value)
        }
        // `from json import dumps` stored as a variable, then passed
        // through map/filter/key=. The eval_call name-lookup branch
        // already calls module dispatch directly for the direct-call
        // form; this arm covers the indirection form.
        Value::ModuleFunction { module, name } => {
            crate::eval::modules::call_function(state, module, name, args, &IndexMap::new(), tools)
                .await
        }
        // Bare builtin function name passed as a value — `try_builtin`
        // is the canonical dispatch; route there with empty kwargs.
        Value::BuiltinName(builtin_name) => {
            // Box::pin breaks the async recursion: try_builtin → min/max →
            // apply_key_fn → call_value_as_function → try_builtin. The
            // future graph is otherwise infinitely-sized at compile time.
            let kwargs = IndexMap::new();
            Box::pin(try_builtin(state, builtin_name, args, &kwargs, tools)).await?.ok_or_else(
                || InterpreterError::TypeError(format!("'{builtin_name}' is not callable")).into(),
            )
        }
        // Bare tool name passed through indirection. resolve_and_dispatch
        // is the canonical entry; we use the same ToolCallDescriptor.
        Value::ToolName(tool_name) => crate::tools::resolver::resolve_and_dispatch(
            state,
            crate::tools::resolver::ToolCallDescriptor {
                name: tool_name,
                args,
                kwargs: &IndexMap::new(),
            },
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
            let kwargs = IndexMap::new();
            let call = CallArgs { positional: args, keyword: &kwargs };
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
        // the bound function. The indirect-call path here doesn't
        // carry call-site kwargs; bound keywords still propagate
        // because the bound function's frame sees them via the
        // module-call shape (kwargs flow into call_user_function
        // via the direct call below). For partial wrapped in another
        // partial (composition), the recursion drops both layers'
        // bound state into the inner call.
        Value::Partial(data) => {
            let bound_args = &data.args;
            let target = &data.func;
            let mut combined: Vec<Value> = Vec::with_capacity(bound_args.len() + args.len());
            combined.extend(bound_args.iter().cloned());
            combined.extend_from_slice(args);
            return Box::pin(call_value_as_function(state, target, &combined, tools)).await;
        }
        Value::LruCache(data) => {
            // Memoize by positional ValueKeys only (kwargs unsupported).
            use crate::eval::literals::value_to_key;
            let key: Result<Vec<_>, _> = args.iter().map(value_to_key).collect();
            let key = key.map_err(|_| {
                InterpreterError::TypeError("lru_cache arguments must be hashable".into())
            })?;
            {
                let mut cache = data.cache.lock();
                if let Some(hit) = cache.get(&key) {
                    // LRU: move hit to end
                    let hit = hit.clone();
                    cache.shift_remove(&key);
                    cache.insert(key.clone(), hit.clone());
                    return Ok(hit);
                }
            }
            let result = Box::pin(call_value_as_function(state, &data.func, args, tools)).await?;
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
                let kwargs = IndexMap::new();
                let call = CallArgs { positional: args, keyword: &kwargs };
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
