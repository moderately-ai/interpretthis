// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `itertools` module.
//!
//! All functions materialise their result as a list — same eager
//! shape as Track C's generator support. Infinite iterators
//! (`count`, `cycle`, `repeat` without a count) bound themselves
//! against the interpreter's operation counter rather than running
//! forever; explicit small bounds give predictable behaviour.

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{control_flow::iterate_value, modules::need_arg},
    value::{Value, shared_list},
};

pub fn has_function(name: &str) -> bool {
    matches!(
        name,
        "chain"
            | "combinations"
            | "permutations"
            | "product"
            | "repeat"
            | "count"
            | "cycle"
            | "islice"
            | "takewhile"
            | "dropwhile"
            | "compress"
            | "accumulate"
    )
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        "chain" => {
            // chain(*iterables) — concatenate every iterable arg.
            let mut out: Vec<Value> = Vec::new();
            for arg in args {
                out.extend(iterate_value(arg)?);
            }
            Ok(Value::List(shared_list(out)))
        }
        "repeat" => {
            // repeat(obj, [times]) — bounded by `times`; unbounded
            // form rejected to keep the eager shape safe.
            let obj = need_arg(func, args, 0)?.clone();
            let times = match args.get(1) {
                Some(Value::Int(n)) => usize::try_from(*n).unwrap_or(0),
                Some(Value::Bool(b)) => usize::from(*b),
                None => {
                    return Err(InterpreterError::Runtime(
                        "itertools.repeat without a count is not supported (would not terminate); pass a `times` argument".into(),
                    )
                    .into());
                }
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "repeat() times must be an integer (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            Ok(Value::List(shared_list(std::iter::repeat_n(obj, times).collect())))
        }
        "count" => {
            // count(start, [step]) — unbounded; we reject because
            // there's no terminating condition. User code should use
            // range() instead.
            Err(InterpreterError::Runtime(
                "itertools.count is not supported (would not terminate); use range() for bounded counters".into(),
            )
            .into())
        }
        "cycle" => Err(InterpreterError::Runtime(
            "itertools.cycle is not supported (would not terminate)".into(),
        )
        .into()),
        "islice" => {
            // islice(iterable, stop) or islice(iterable, start, stop, [step])
            let iter_arg = need_arg(func, args, 0)?;
            let items = iterate_value(iter_arg)?;
            let (start, stop, step) = match args.len() {
                2 => (0usize, opt_usize(args, 1).unwrap_or(items.len()), 1usize),
                3 => {
                    (opt_usize(args, 1).unwrap_or(0), opt_usize(args, 2).unwrap_or(items.len()), 1)
                }
                4 => (
                    opt_usize(args, 1).unwrap_or(0),
                    opt_usize(args, 2).unwrap_or(items.len()),
                    opt_usize(args, 3).unwrap_or(1).max(1),
                ),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "islice() requires 2-4 arguments".into(),
                    )
                    .into());
                }
            };
            let stop = stop.min(items.len());
            let mut out = Vec::new();
            let mut idx = start;
            while idx < stop {
                out.push(items[idx].clone());
                idx += step;
            }
            Ok(Value::List(shared_list(out)))
        }
        "combinations" => {
            // combinations(iterable, r) — all r-length tuples in
            // lexicographic order, no repeats.
            let items = iterate_value(need_arg(func, args, 0)?)?;
            let r = arg_usize("combinations", args, 1)?;
            Ok(Value::List(shared_list(combinations(&items, r))))
        }
        "permutations" => {
            // permutations(iterable, [r]) — all r-length permutations.
            let items = iterate_value(need_arg(func, args, 0)?)?;
            let r = match args.get(1) {
                Some(Value::Int(n)) => usize::try_from(*n).unwrap_or(items.len()),
                Some(Value::None) | None => items.len(),
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "permutations() r must be an integer (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            Ok(Value::List(shared_list(permutations(&items, r))))
        }
        "product" => {
            // product(*iterables, repeat=1) — Cartesian product.
            // repeat kwarg not threaded; users wanting it write
            // product(a, a, a).
            let pools: Vec<Vec<Value>> =
                args.iter().map(iterate_value).collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(shared_list(cartesian_product(&pools))))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'itertools' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// `combinations(items, r)`: all r-length sub-tuples in
/// lexicographic order.
fn combinations(items: &[Value], r: usize) -> Vec<Value> {
    if r > items.len() {
        return Vec::new();
    }
    if r == 0 {
        return vec![Value::Tuple(Vec::new())];
    }
    let mut result = Vec::new();
    let n = items.len();
    let mut indices: Vec<usize> = (0..r).collect();
    loop {
        let combo: Vec<Value> = indices.iter().map(|&i| items[i].clone()).collect();
        result.push(Value::Tuple(combo));
        // Advance: find rightmost index that can be incremented.
        let mut i = r;
        while i > 0 {
            i -= 1;
            if indices[i] != i + n - r {
                indices[i] += 1;
                for j in (i + 1)..r {
                    indices[j] = indices[j - 1] + 1;
                }
                break;
            }
            if i == 0 {
                return result;
            }
        }
    }
}

/// `permutations(items, r)`: all r-length orderings.
fn permutations(items: &[Value], r: usize) -> Vec<Value> {
    if r > items.len() {
        return Vec::new();
    }
    if r == 0 {
        return vec![Value::Tuple(Vec::new())];
    }
    let mut result = Vec::new();
    let n = items.len();
    let mut indices: Vec<usize> = (0..n).collect();
    let mut cycles: Vec<usize> = (n - r + 1..=n).rev().collect();
    result.push(Value::Tuple(indices.iter().take(r).map(|&i| items[i].clone()).collect()));
    loop {
        let mut done = true;
        for i in (0..r).rev() {
            cycles[i] -= 1;
            if cycles[i] == 0 {
                let removed = indices.remove(i);
                indices.push(removed);
                cycles[i] = n - i;
            } else {
                let j = indices.len() - cycles[i];
                indices.swap(i, j);
                result.push(Value::Tuple(
                    indices.iter().take(r).map(|&k| items[k].clone()).collect(),
                ));
                done = false;
                break;
            }
        }
        if done {
            return result;
        }
    }
}

/// `product(*pools)`: Cartesian product.
fn cartesian_product(pools: &[Vec<Value>]) -> Vec<Value> {
    if pools.is_empty() {
        return vec![Value::Tuple(Vec::new())];
    }
    if pools.iter().any(Vec::is_empty) {
        return Vec::new();
    }
    let mut result = vec![Vec::new()];
    for pool in pools {
        let mut next = Vec::new();
        for combo in &result {
            for item in pool {
                let mut extended = combo.clone();
                extended.push(item.clone());
                next.push(extended);
            }
        }
        result = next;
    }
    result.into_iter().map(Value::Tuple).collect()
}

fn opt_usize(args: &[Value], index: usize) -> Option<usize> {
    match args.get(index)? {
        Value::Int(n) => usize::try_from(*n).ok(),
        Value::Bool(b) => Some(usize::from(*b)),
        // Value::None and any non-integer falls through to None — the
        // None case is documented behaviour (islice accepts None for
        // unbounded start/stop), other shapes are rejected by the
        // caller via TypeError if the slot was required.
        _ => None,
    }
}

fn arg_usize(func: &str, args: &[Value], index: usize) -> Result<usize, EvalError> {
    match args.get(index) {
        Some(Value::Int(n)) => usize::try_from(*n).map_err(|_| {
            EvalError::from(InterpreterError::ValueError(format!("{func}() argument out of range")))
        }),
        Some(Value::Bool(b)) => Ok(usize::from(*b)),
        _ => Err(InterpreterError::TypeError(format!(
            "{func}() missing or non-integer argument at position {index}"
        ))
        .into()),
    }
}

/// `compress(data, selectors)`: yield `data[i]` for each `selectors[i]`
/// that is truthy. Eager: materialises the entire output list.
fn compress_impl(args: &[Value]) -> EvalResult {
    let data_val = need_arg("compress", args, 0)?;
    let selectors_val = need_arg("compress", args, 1)?;
    let data = iterate_value(data_val)?;
    let selectors = iterate_value(selectors_val)?;
    let mut out = Vec::new();
    for (item, sel) in data.into_iter().zip(selectors) {
        if sel.is_truthy() {
            out.push(item);
        }
    }
    Ok(Value::List(shared_list(out)))
}

/// `takewhile(predicate, iterable)`: yield items until predicate returns
/// falsy. Re-enters the evaluator for each predicate call.
async fn takewhile_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let pred = need_arg("takewhile", args, 0)?.clone();
    let iter_val = need_arg("takewhile", args, 1)?;
    let items = iterate_value(iter_val)?;
    let kwargs = indexmap::IndexMap::new();
    let mut out = Vec::new();
    for item in items {
        let verdict = crate::eval::modules::call_callable(
            state,
            &pred,
            std::slice::from_ref(&item),
            &kwargs,
            tools,
        )
        .await?;
        if !verdict.is_truthy() {
            break;
        }
        out.push(item);
    }
    Ok(Value::List(shared_list(out)))
}

/// `dropwhile(predicate, iterable)`: drop items while predicate returns
/// truthy; yield all remaining items unconditionally once the predicate
/// has fired False for the first time.
async fn dropwhile_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let pred = need_arg("dropwhile", args, 0)?.clone();
    let iter_val = need_arg("dropwhile", args, 1)?;
    let items = iterate_value(iter_val)?;
    let kwargs = indexmap::IndexMap::new();
    let mut out = Vec::new();
    let mut dropping = true;
    for item in items {
        if dropping {
            let verdict = crate::eval::modules::call_callable(
                state,
                &pred,
                std::slice::from_ref(&item),
                &kwargs,
                tools,
            )
            .await?;
            if verdict.is_truthy() {
                continue;
            }
            dropping = false;
        }
        out.push(item);
    }
    Ok(Value::List(shared_list(out)))
}

/// `accumulate(iterable, [func=operator.add])`: cumulative reductions.
/// Default reducer is addition; passing a callable folds via it.
async fn accumulate_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let iter_val = need_arg("accumulate", args, 0)?;
    let items = iterate_value(iter_val)?;
    let reducer = args.get(1).cloned();
    let kwargs = indexmap::IndexMap::new();
    let mut out: Vec<Value> = Vec::new();
    let mut acc: Option<Value> = None;
    for item in items {
        acc = Some(match acc {
            None => item,
            Some(prev) => match &reducer {
                Some(callable) => {
                    crate::eval::modules::call_callable(
                        state,
                        callable,
                        &[prev, item],
                        &kwargs,
                        tools,
                    )
                    .await?
                }
                None => crate::types::dispatch_binop(
                    crate::types::BinOp::Add,
                    &prev,
                    &item,
                    state.decimal_prec,
                )?,
            },
        });
        if let Some(v) = &acc {
            out.push(v.clone());
        }
    }
    Ok(Value::List(shared_list(out)))
}

/// `itertools` module registration. Predicate-driven functions
/// (`takewhile`, `dropwhile`, `accumulate`) re-enter the evaluator to
/// invoke the user-supplied callable; eager ones (`chain`, `repeat`,
/// `combinations`, ...) route to the sync `call` body.
pub struct ItertoolsModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for ItertoolsModule {
    fn name(&self) -> &'static str {
        "itertools"
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &indexmap::IndexMap<String, Value>,
        tools: &crate::tools::Tools,
    ) -> EvalResult {
        match func {
            "takewhile" => takewhile_impl(state, args, tools).await,
            "dropwhile" => dropwhile_impl(state, args, tools).await,
            "accumulate" => accumulate_impl(state, args, tools).await,
            "compress" => compress_impl(args),
            _ => call(func, args),
        }
    }
}
