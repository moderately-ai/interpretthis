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
    eval::{
        control_flow::iterate_value,
        modules::{need_arg, value_error},
    },
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
            | "groupby"
    )
}

pub fn call(func: &str, args: &[Value], kwargs: &indexmap::IndexMap<String, Value>) -> EvalResult {
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
            // islice(iterable, stop) or islice(iterable, start, stop, [step]).
            // Bounds are validated: start/stop must be a non-negative int or
            // None, step a positive int or None (CPython raises ValueError).
            let items = iterate_value(need_arg(func, args, 0)?)?;
            let (start, stop, step) = match args.len() {
                2 => (0usize, islice_bound(args.get(1), items.len(), "Stop")?, 1usize),
                3 => (
                    islice_bound(args.get(1), 0, "Start")?,
                    islice_bound(args.get(2), items.len(), "Stop")?,
                    1usize,
                ),
                4 => (
                    islice_bound(args.get(1), 0, "Start")?,
                    islice_bound(args.get(2), items.len(), "Stop")?,
                    islice_step(args.get(3))?,
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
                None | Some(Value::None) => items.len(),
                Some(Value::Bool(b)) => usize::from(*b),
                // A negative r is a ValueError (not "treat as len"); a huge r
                // that overflows usize is likewise out of range.
                Some(Value::Int(n)) => usize::try_from(*n)
                    .map_err(|_| value_error("permutations() r must be non-negative"))?,
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
            // product(*iterables, repeat=1) — Cartesian product of the pools
            // repeated `repeat` times (`product(a, b, repeat=2)` pools are
            // [a, b, a, b]).
            let repeat = match kwargs.get("repeat") {
                None => 1,
                Some(Value::Int(n)) => usize::try_from(*n)
                    .map_err(|_| value_error("product() repeat must be non-negative"))?,
                Some(Value::Bool(b)) => usize::from(*b),
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "product() repeat must be an integer (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            if let Some(bad) = kwargs.keys().find(|k| k.as_str() != "repeat") {
                return Err(InterpreterError::TypeError(format!(
                    "product() got an unexpected keyword argument '{bad}'"
                ))
                .into());
            }
            let base: Vec<Vec<Value>> =
                args.iter().map(iterate_value).collect::<Result<Vec<_>, _>>()?;
            let pools: Vec<Vec<Value>> = std::iter::repeat_n(base, repeat).flatten().collect();
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

/// An `islice` start/stop bound: absent or `None` uses `default`; a
/// non-negative integer is that value; anything else (negative, float, ...)
/// raises `ValueError`, matching CPython.
fn islice_bound(arg: Option<&Value>, default: usize, which: &str) -> Result<usize, EvalError> {
    match arg {
        None | Some(Value::None) => Ok(default),
        Some(Value::Int(n)) if *n >= 0 => Ok(usize::try_from(*n).unwrap_or(usize::MAX)),
        Some(Value::Bool(b)) => Ok(usize::from(*b)),
        _ => Err(value_error(format!(
            "{which} argument for islice() must be None or an integer: 0 <= x <= sys.maxsize."
        ))),
    }
}

/// The `islice` step: absent or `None` is 1; otherwise a positive integer.
/// Zero, negative, or non-integer raises `ValueError`.
fn islice_step(arg: Option<&Value>) -> Result<usize, EvalError> {
    match arg {
        None | Some(Value::None) => Ok(1),
        Some(Value::Int(n)) if *n >= 1 => Ok(usize::try_from(*n).unwrap_or(usize::MAX)),
        Some(Value::Bool(true)) => Ok(1),
        _ => Err(value_error("Step for islice() must be a positive integer or None.")),
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
/// `groupby(iterable, key=None)`: group consecutive elements sharing a key.
/// Yields `(key, group)` pairs; in our eager model the group materialises as a
/// list. `key=None` groups by the elements themselves. Only *consecutive* runs
/// group, matching CPython (callers `sorted()` first for a full grouping).
async fn groupby_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
    tools: &crate::tools::Tools,
) -> EvalResult {
    let items = iterate_value(need_arg("groupby", args, 0)?)?;
    let key_fn =
        args.get(1).or_else(|| kwargs.get("key")).filter(|v| !matches!(v, Value::None)).cloned();
    let empty = indexmap::IndexMap::new();
    let mut out: Vec<Value> = Vec::new();
    let mut current: Option<(Value, Vec<Value>)> = None;
    for item in items {
        let key = match &key_fn {
            Some(f) => {
                crate::eval::modules::call_callable(
                    state,
                    f,
                    std::slice::from_ref(&item),
                    &empty,
                    tools,
                )
                .await?
            }
            None => item.clone(),
        };
        match &mut current {
            Some((ck, group)) if crate::eval::operations::values_equal_pub(ck, &key) => {
                group.push(item);
            }
            _ => {
                if let Some((ck, group)) = current.take() {
                    out.push(Value::Tuple(vec![ck, Value::List(shared_list(group))]));
                }
                current = Some((key, vec![item]));
            }
        }
    }
    if let Some((ck, group)) = current {
        out.push(Value::Tuple(vec![ck, Value::List(shared_list(group))]));
    }
    Ok(Value::List(shared_list(out)))
}

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
    call_kwargs: &indexmap::IndexMap<String, Value>,
    tools: &crate::tools::Tools,
) -> EvalResult {
    let iter_val = need_arg("accumulate", args, 0)?;
    let items = iterate_value(iter_val)?;
    let reducer = args.get(1).cloned();
    let kwargs = indexmap::IndexMap::new();
    let mut out: Vec<Value> = Vec::new();
    // `initial=`: seed the accumulator and emit it first, so the output has one
    // more element than the input (CPython 3.8+).
    let mut acc: Option<Value> = match call_kwargs.get("initial") {
        None | Some(Value::None) => None,
        Some(seed) => {
            out.push(seed.clone());
            Some(seed.clone())
        }
    };
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
        kwargs: &indexmap::IndexMap<String, Value>,
        tools: &crate::tools::Tools,
    ) -> EvalResult {
        match func {
            "takewhile" => takewhile_impl(state, args, tools).await,
            "dropwhile" => dropwhile_impl(state, args, tools).await,
            "accumulate" => accumulate_impl(state, args, kwargs, tools).await,
            "compress" => compress_impl(args),
            "groupby" => groupby_impl(state, args, kwargs, tools).await,
            _ => call(func, args, kwargs),
        }
    }
}
