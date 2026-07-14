// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use rustpython_parser::ast::{self, Expr};

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{
        eval_expr,
        functions::{VariableCheckpoint, resolve_proxy},
        literals::value_to_key,
    },
    state::InterpreterState,
    tools::Tools,
    value::{Value, ValueKey, shared_list},
};

/// Collect every name introduced by a comprehension's `for X in ...`
/// generator targets (e.g. `x, y` in `[... for x, y in pairs]`). These
/// names are scoped to the comprehension in CPython; in our flat-state
/// model we snapshot their pre-comp values and restore on exit so the
/// names don't leak. Walrus targets are deliberately NOT collected
/// here — PEP 572 binds them to the comprehension's *enclosing* scope.
fn collect_generator_target_names(generators: &[ast::Comprehension]) -> Vec<String> {
    let mut names = Vec::new();
    for g in generators {
        collect_target_names(&g.target, &mut names);
    }
    names
}

/// Same `collect_target_names` walker the function-frame checkpoint uses,
/// inlined here so the comprehension module doesn't need a public export
/// from the `functions` module. Kept private — this isn't a stable API.
fn collect_target_names(target: &ast::Expr, out: &mut Vec<String>) {
    match target {
        Expr::Name(n) => {
            let s = n.id.as_str().to_string();
            if !out.contains(&s) {
                out.push(s);
            }
        }
        Expr::Tuple(t) => {
            for e in &t.elts {
                collect_target_names(e, out);
            }
        }
        Expr::List(l) => {
            for e in &l.elts {
                collect_target_names(e, out);
            }
        }
        Expr::Starred(s) => collect_target_names(&s.value, out),
        _ => {}
    }
}

/// Evaluate a list comprehension [expr for x in iterable if cond].
///
/// The comprehension target names are scoped to the comprehension —
/// snapshotted on entry and restored on exit. Walrus targets inside
/// the comprehension are deliberately NOT included in the checkpoint,
/// so they propagate to the enclosing function scope (PEP 572).
pub async fn eval_list_comp(
    state: &mut InterpreterState,
    node: &ast::ExprListComp,
    tools: &Tools,
) -> EvalResult {
    let checkpoint =
        VariableCheckpoint::capture(state, collect_generator_target_names(&node.generators));
    let mut results = Vec::new();

    let outcome = eval_list_generators(ListGenContext {
        state,
        generators: &node.generators,
        index: 0,
        elt_expr: &node.elt,
        results: &mut results,
        tools,
    })
    .await;

    checkpoint.restore(state);
    outcome?;

    Ok(Value::List(shared_list(results)))
}

/// Evaluate a generator expression `(expr for x in iterable if cond)`.
///
/// The interpreter has no coroutine/`yield` machinery, so a generator is
/// materialised eagerly into a `Value::List`. Every consumer in this sandbox
/// (`sum`/`all`/`any`/`min`/`max`/`sorted`/`list`/`set` and `for` loops) treats
/// the result as a plain iterable, so eager materialisation is observably
/// identical to lazy iteration for bounded inputs — and the operation/loop
/// limits already bound the input. The cost is that side effects run at
/// construction time rather than on demand and that unbounded generators are
/// not representable; both are acceptable in a sandbox that forbids I/O and
/// caps iteration counts.
pub async fn eval_generator_exp(
    state: &mut InterpreterState,
    node: &ast::ExprGeneratorExp,
    tools: &Tools,
) -> EvalResult {
    let checkpoint =
        VariableCheckpoint::capture(state, collect_generator_target_names(&node.generators));
    let mut results = Vec::new();

    let outcome = eval_list_generators(ListGenContext {
        state,
        generators: &node.generators,
        index: 0,
        elt_expr: &node.elt,
        results: &mut results,
        tools,
    })
    .await;

    checkpoint.restore(state);
    outcome?;

    // A generator expression is a one-shot lazy iterator, not a list: `next(g)`
    // advances it and a later `list(g)` yields only the remainder. We eagerly
    // materialise the items (the sandbox caps iteration and forbids unbounded
    // streams) but wrap them in the `Lazy` cursor type so the iterator protocol
    // (`next`, single-pass `for`/`list`/`sum`) behaves as CPython's does.
    let cursor_id = state.next_cursor_id;
    state.next_cursor_id = state.next_cursor_id.wrapping_add(1);
    state.lazy_cursors.insert(cursor_id, 0);
    Ok(Value::Lazy { items: results, cursor_id })
}

/// Evaluate a dict comprehension {key: val for x in iterable if cond}.
pub async fn eval_dict_comp(
    state: &mut InterpreterState,
    node: &ast::ExprDictComp,
    tools: &Tools,
) -> EvalResult {
    let checkpoint =
        VariableCheckpoint::capture(state, collect_generator_target_names(&node.generators));
    let mut result_map = indexmap::IndexMap::new();

    let outcome = eval_dict_generators(DictGenContext {
        state,
        generators: &node.generators,
        index: 0,
        key_expr: &node.key,
        value_expr: &node.value,
        result_map: &mut result_map,
        tools,
    })
    .await;

    checkpoint.restore(state);
    outcome?;

    Ok(Value::Dict(result_map))
}

/// Evaluate a set comprehension {expr for x in iterable if cond}.
pub async fn eval_set_comp(
    state: &mut InterpreterState,
    node: &ast::ExprSetComp,
    tools: &Tools,
) -> EvalResult {
    let checkpoint =
        VariableCheckpoint::capture(state, collect_generator_target_names(&node.generators));
    let mut results = Vec::new();

    let outcome = eval_list_generators(ListGenContext {
        state,
        generators: &node.generators,
        index: 0,
        elt_expr: &node.elt,
        results: &mut results,
        tools,
    })
    .await;

    checkpoint.restore(state);
    outcome?;

    // Shared set construction: raises on an unhashable element and dedups
    // instances by __eq__. The old open-coded `value_to_key(x).ok()` dedup
    // silently dropped every element after the first unhashable one (all
    // compared equal as `None`), losing data instead of raising.
    crate::eval::literals::build_set(state, results, tools).await
}

/// Per-call context for [`eval_list_generators`].
struct ListGenContext<'a> {
    state: &'a mut InterpreterState,
    generators: &'a [ast::Comprehension],
    index: usize,
    elt_expr: &'a Expr,
    results: &'a mut Vec<Value>,
    tools: &'a Tools,
}

/// Recursively evaluate generators for list/set comprehensions.
fn eval_list_generators<'a>(
    ctx: ListGenContext<'a>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EvalError>> + Send + 'a>> {
    Box::pin(async move {
        let ListGenContext { state, generators, index, elt_expr, results, tools } = ctx;

        if index >= generators.len() {
            // Base case: evaluate the element expression
            let val = eval_expr(state, elt_expr, tools).await?;
            results.push(val);
            return Ok(());
        }

        let generator = &generators[index];
        let iterable = eval_expr(state, &generator.iter, tools).await?;
        let iterable = resolve_proxy(&iterable).await?;

        // Range fast path: walk (start, stop, step) without materializing
        // 10k Value::Int entries the iter consumer would immediately drop.
        // `[x * x for x in range(10000)]` is the canonical case.
        if let Value::Range { start, stop, step } = iterable {
            let pos = step > 0;
            let mut i = start;
            loop {
                let in_range = (pos && i < stop) || (step < 0 && i > stop);
                if !in_range {
                    break;
                }
                set_comprehension_target(state, &generator.target, &Value::Int(i)).await?;

                let mut include = true;
                for if_clause in &generator.ifs {
                    let cond = eval_expr(state, if_clause, tools).await?;
                    let cond = resolve_proxy(&cond).await?;
                    if !crate::eval::op::truthy(state, &cond, tools).await? {
                        include = false;
                        break;
                    }
                }
                if include {
                    eval_list_generators(ListGenContext {
                        state,
                        generators,
                        index: index + 1,
                        elt_expr,
                        results,
                        tools,
                    })
                    .await?;
                }

                let Some(next) = i.checked_add(step) else { break };
                i = next;
            }
            return Ok(());
        }

        let items = crate::eval::op::iter(state, &iterable, tools).await?;

        for item in items {
            // Set the target variable
            set_comprehension_target(state, &generator.target, &item).await?;

            // Check if-filters
            let mut include = true;
            for if_clause in &generator.ifs {
                let cond = eval_expr(state, if_clause, tools).await?;
                let cond = resolve_proxy(&cond).await?;
                if !crate::eval::op::truthy(state, &cond, tools).await? {
                    include = false;
                    break;
                }
            }

            if include {
                eval_list_generators(ListGenContext {
                    state,
                    generators,
                    index: index + 1,
                    elt_expr,
                    results,
                    tools,
                })
                .await?;
            }
        }

        Ok(())
    })
}

/// Per-call context for [`eval_dict_generators`].
///
/// `key_expr` and `value_expr` are both `&Expr`; without bundling, a
/// silent transposition would flip every comprehension's key/value
/// pair. The struct makes the role of each `Expr` named at every
/// recursive call site.
struct DictGenContext<'a> {
    state: &'a mut InterpreterState,
    generators: &'a [ast::Comprehension],
    index: usize,
    key_expr: &'a Expr,
    value_expr: &'a Expr,
    result_map: &'a mut indexmap::IndexMap<ValueKey, Value>,
    tools: &'a Tools,
}

/// Recursively evaluate generators for dict comprehensions.
fn eval_dict_generators<'a>(
    ctx: DictGenContext<'a>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EvalError>> + Send + 'a>> {
    Box::pin(async move {
        let DictGenContext { state, generators, index, key_expr, value_expr, result_map, tools } =
            ctx;

        if index >= generators.len() {
            let key = eval_expr(state, key_expr, tools).await?;
            let value = eval_expr(state, value_expr, tools).await?;
            // Instance keys go through the async hash/`__eq__` protocol
            // (same as a dict literal); other keys use the sync
            // `value_to_key`. Previously the comprehension always called
            // `value_to_key`, which rejects instances as unhashable.
            if matches!(key, Value::Instance(_)) {
                crate::eval::op::dict_insert_instance_key_pub(
                    state, result_map, &key, value, tools,
                )
                .await?;
            } else {
                result_map.insert(value_to_key(&key)?, value);
            }
            return Ok(());
        }

        let generator = &generators[index];
        let iterable = eval_expr(state, &generator.iter, tools).await?;
        let iterable = resolve_proxy(&iterable).await?;

        // Range fast path: same rationale as the list-comp variant above.
        if let Value::Range { start, stop, step } = iterable {
            let pos = step > 0;
            let mut i = start;
            loop {
                let in_range = (pos && i < stop) || (step < 0 && i > stop);
                if !in_range {
                    break;
                }
                set_comprehension_target(state, &generator.target, &Value::Int(i)).await?;

                let mut include = true;
                for if_clause in &generator.ifs {
                    let cond = eval_expr(state, if_clause, tools).await?;
                    let cond = resolve_proxy(&cond).await?;
                    if !crate::eval::op::truthy(state, &cond, tools).await? {
                        include = false;
                        break;
                    }
                }
                if include {
                    eval_dict_generators(DictGenContext {
                        state,
                        generators,
                        index: index + 1,
                        key_expr,
                        value_expr,
                        result_map,
                        tools,
                    })
                    .await?;
                }

                let Some(next) = i.checked_add(step) else { break };
                i = next;
            }
            return Ok(());
        }

        let items = crate::eval::op::iter(state, &iterable, tools).await?;

        for item in items {
            set_comprehension_target(state, &generator.target, &item).await?;

            let mut include = true;
            for if_clause in &generator.ifs {
                let cond = eval_expr(state, if_clause, tools).await?;
                let cond = resolve_proxy(&cond).await?;
                if !crate::eval::op::truthy(state, &cond, tools).await? {
                    include = false;
                    break;
                }
            }

            if include {
                eval_dict_generators(DictGenContext {
                    state,
                    generators,
                    index: index + 1,
                    key_expr,
                    value_expr,
                    result_map,
                    tools,
                })
                .await?;
            }
        }

        Ok(())
    })
}

/// Set a comprehension target variable (handles simple names and tuple unpacking).
fn set_comprehension_target<'a>(
    state: &'a mut InterpreterState,
    target: &'a Expr,
    value: &'a Value,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EvalError>> + Send + 'a>> {
    Box::pin(async move {
        match target {
            Expr::Name(name_node) => {
                // Comprehension targets are comp-scoped (Python 3
                // semantics) and overwritten every iteration. The full
                // `set_variable` path runs memory accounting (estimate
                // size of old + new value) and a nonlocal-cell
                // write-through check that's irrelevant for comp
                // targets — both are pure overhead per element. Direct
                // map insert is correct: the result-accumulator's
                // memory IS tracked (the .push into results), only the
                // per-element target's churn is skipped.
                state.variables.insert(name_node.id.as_str().to_string(), value.clone());
                Ok(())
            }
            Expr::Tuple(tuple_node) => {
                let items: Vec<Value> = match value {
                    Value::List(items) => items.lock().clone(),
                    Value::Tuple(items) => items.clone(),
                    Value::String(s) => {
                        s.chars().map(|c| Value::String(c.to_string().into())).collect()
                    }
                    _ => {
                        return Err(InterpreterError::TypeError(
                            "cannot unpack non-iterable value in comprehension".into(),
                        )
                        .into());
                    }
                };

                if tuple_node.elts.len() != items.len() {
                    return Err(InterpreterError::Runtime(
                        "cannot unpack tuple of wrong size in comprehension".into(),
                    )
                    .into());
                }

                for (elem, val) in tuple_node.elts.iter().zip(items.iter()) {
                    set_comprehension_target(state, elem, val).await?;
                }
                Ok(())
            }
            _ => Err(InterpreterError::Runtime(format!(
                "unsupported comprehension target: {:?}",
                std::mem::discriminant(target)
            ))
            .into()),
        }
    })
}
