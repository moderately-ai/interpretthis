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
            | "combinations_with_replacement"
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
            | "zip_longest"
            | "starmap"
            | "pairwise"
            | "filterfalse"
            | "tee"
            | "batched"
            | "chain.from_iterable"
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
        "chain.from_iterable" => {
            // chain.from_iterable(iterable) — flatten one level: iterate the
            // single argument and concatenate each sub-iterable in order.
            let mut out: Vec<Value> = Vec::new();
            for sub in iterate_value(need_arg("chain.from_iterable", args, 0)?)? {
                out.extend(iterate_value(&sub)?);
            }
            Ok(Value::List(shared_list(out)))
        }
        "batched" => {
            // batched(iterable, n) — consecutive n-length tuples; the final
            // batch is short. n must be at least 1 (CPython 3.12+).
            let items = iterate_value(need_arg("batched", args, 0)?)?;
            let n = arg_usize("batched", args, 1)?;
            if n == 0 {
                return Err(InterpreterError::ValueError("n must be at least one".into()).into());
            }
            let batches = items.chunks(n).map(|c| Value::Tuple(c.to_vec())).collect::<Vec<_>>();
            Ok(Value::List(shared_list(batches)))
        }
        "tee" => {
            // tee(iterable, n=2) — n independent iterators over the same items.
            // The eager model returns n separate list copies (matching how
            // `chain` etc. materialise), so each iterates independently.
            let items = iterate_value(need_arg(func, args, 0)?)?;
            let n = match args.get(1) {
                None => 2,
                Some(Value::Bool(b)) => usize::from(*b),
                Some(Value::Int(k)) if *k >= 0 => usize::try_from(*k).unwrap_or(0),
                Some(Value::Int(_)) => {
                    return Err(InterpreterError::ValueError("n must be >= 0".into()).into());
                }
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "tee() n must be an integer (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            let copies = (0..n).map(|_| Value::List(shared_list(items.clone()))).collect();
            Ok(Value::Tuple(copies))
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
                        "itertools.repeat without a count is not supported (would not terminate); pass a `times` argument (see CONFORMANCE.md#unsupported-language-features)".into(),
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
        // `count` / `cycle` are the unbounded lazy producers; they need
        // interpreter state to register their cursor, so they are
        // handled in the async `Module::call` and never reach here.
        "count" | "cycle" => Err(InterpreterError::Runtime(format!(
            "itertools.{func} must be evaluated through the module dispatch"
        ))
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
        "combinations_with_replacement" => {
            // Like combinations, but an element may be chosen more than once.
            let items = iterate_value(need_arg(func, args, 0)?)?;
            let r = arg_usize("combinations_with_replacement", args, 1)?;
            Ok(Value::List(shared_list(combinations_with_replacement(&items, r))))
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
        "zip_longest" => {
            // zip_longest(*iterables, fillvalue=None) — zip to the
            // longest input, padding short ones with `fillvalue`.
            if let Some(bad) = kwargs.keys().find(|k| k.as_str() != "fillvalue") {
                return Err(InterpreterError::TypeError(format!(
                    "zip_longest() got an unexpected keyword argument '{bad}'"
                ))
                .into());
            }
            let fill = kwargs.get("fillvalue").cloned().unwrap_or(Value::None);
            let pools: Vec<Vec<Value>> =
                args.iter().map(iterate_value).collect::<Result<Vec<_>, _>>()?;
            let max_len = pools.iter().map(Vec::len).max().unwrap_or(0);
            let mut out = Vec::with_capacity(max_len);
            for i in 0..max_len {
                let row: Vec<Value> = pools
                    .iter()
                    .map(|p| p.get(i).cloned().unwrap_or_else(|| fill.clone()))
                    .collect();
                out.push(Value::Tuple(row));
            }
            Ok(Value::List(shared_list(out)))
        }
        "pairwise" => {
            // pairwise(iterable) — successive overlapping pairs.
            let items = iterate_value(need_arg(func, args, 0)?)?;
            let out: Vec<Value> =
                items.windows(2).map(|w| Value::Tuple(vec![w[0].clone(), w[1].clone()])).collect();
            Ok(Value::List(shared_list(out)))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'itertools' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// `combinations(items, r)`: all r-length sub-tuples in
/// lexicographic order.
/// `combinations_with_replacement(iterable, r)` — sorted r-length tuples where
/// each element may repeat (indices are non-decreasing, all 0..n each usable).
fn combinations_with_replacement(items: &[Value], r: usize) -> Vec<Value> {
    let n = items.len();
    if n == 0 {
        return if r == 0 { vec![Value::Tuple(Vec::new())] } else { Vec::new() };
    }
    let mut result = Vec::new();
    let mut indices = vec![0usize; r];
    loop {
        result.push(Value::Tuple(indices.iter().map(|&i| items[i].clone()).collect()));
        // Advance: rightmost index not yet at n-1 increments; the rest reset to it.
        let mut i = r;
        loop {
            if i == 0 {
                return result;
            }
            i -= 1;
            if indices[i] != n - 1 {
                let v = indices[i] + 1;
                for slot in &mut indices[i..] {
                    *slot = v;
                }
                break;
            }
        }
    }
}

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
/// `islice(iterable, stop)` / `islice(iterable, start, stop[, step])` with lazy
/// consumption for generator/lazy-iterator inputs — it pulls at most `stop`
/// items via the generator's single-step primitive rather than materialising
/// the whole (possibly infinite) input. Non-lazy inputs use the eager path.
async fn islice_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let input = need_arg("islice", args, 0)?;
    if !matches!(input, Value::Generator { .. } | Value::Lazy { .. } | Value::BuiltinIter { .. }) {
        return call("islice", args, &indexmap::IndexMap::new());
    }
    let bound = |v: Option<&Value>| -> Result<Option<usize>, EvalError> {
        match v {
            None | Some(Value::None) => Ok(None),
            Some(Value::Int(n)) if *n >= 0 => Ok(Some(usize::try_from(*n).unwrap_or(usize::MAX))),
            Some(Value::Bool(b)) => Ok(Some(usize::from(*b))),
            _ => Err(value_error(
                "Indices for islice() must be None or an integer: 0 <= x <= sys.maxsize.",
            )),
        }
    };
    let (start, stop, step) = match args.len() {
        2 => (0usize, bound(args.get(1))?, 1usize),
        3 => (bound(args.get(1))?.unwrap_or(0), bound(args.get(2))?, 1usize),
        4 => {
            let step = match bound(args.get(3))? {
                Some(0) => {
                    return Err(value_error(
                        "Step for islice() must be a positive integer or None.",
                    ));
                }
                Some(s) => s,
                None => 1,
            };
            (bound(args.get(1))?.unwrap_or(0), bound(args.get(2))?, step)
        }
        _ => {
            return Err(
                InterpreterError::TypeError("islice() requires 2-4 arguments".into()).into()
            );
        }
    };
    let input = input.clone();
    let empty = indexmap::IndexMap::new();
    let mut out = Vec::new();
    let mut idx = 0usize;
    loop {
        if stop.is_some_and(|s| idx >= s) {
            break;
        }
        let item = match crate::eval::functions::dispatch_generator_method(
            state,
            &input,
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
        if idx >= start && (idx - start) % step == 0 {
            out.push(item);
        }
        idx += 1;
    }
    Ok(Value::List(shared_list(out)))
}

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
    let items = crate::eval::op::iter(state, need_arg("groupby", args, 0)?, tools).await?;
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
    let iter_val = need_arg("takewhile", args, 1)?.clone();
    let kwargs = indexmap::IndexMap::new();
    let mut out = Vec::new();
    // Lazy consumption for a generator input, so `takewhile(pred, infinite())`
    // stops as soon as the predicate fails instead of draining forever.
    if matches!(iter_val, Value::Generator { .. } | Value::Lazy { .. } | Value::BuiltinIter { .. })
    {
        loop {
            let item = match crate::eval::functions::dispatch_generator_method(
                state,
                &iter_val,
                "__next__",
                &[],
                &kwargs,
                tools,
            )
            .await
            {
                Ok(v) => v,
                Err(EvalError::Exception(e)) if e.type_name == "StopIteration" => break,
                Err(e) => return Err(e),
            };
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
        return Ok(Value::List(shared_list(out)));
    }
    for item in iterate_value(&iter_val)? {
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
    let iter_val = need_arg("dropwhile", args, 1)?.clone();
    let items = crate::eval::op::iter(state, &iter_val, tools).await?;
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

/// `count(start=0, step=1)` — register an unbounded counter. `start`
/// and `step` must be numbers (CPython rejects non-numeric with
/// TypeError).
fn make_count(state: &mut crate::state::InterpreterState, args: &[Value]) -> EvalResult {
    let numeric = |v: &Value, what: &str| -> Result<Value, EvalError> {
        match v {
            Value::Int(_) | Value::BigInt(_) | Value::Float(_) | Value::Bool(_) => Ok(v.clone()),
            other => Err(InterpreterError::TypeError(format!(
                "a number is required for count() {what}, not '{}'",
                other.type_name()
            ))
            .into()),
        }
    };
    let start = match args.first() {
        None | Some(Value::None) => Value::Int(0),
        Some(v) => numeric(v, "start")?,
    };
    let step = match args.get(1) {
        None | Some(Value::None) => Value::Int(1),
        Some(v) => numeric(v, "step")?,
    };
    Ok(state.alloc_builtin_iter(
        crate::value::BuiltinIterName::Count,
        crate::state::BuiltinIterState::Count { next: start, step },
    ))
}

/// `cycle(iterable)` — buffer the (finite) input eagerly, then repeat it
/// forever.
fn make_cycle(state: &mut crate::state::InterpreterState, args: &[Value]) -> EvalResult {
    let items = iterate_value(need_arg("cycle", args, 0)?)?;
    Ok(state.alloc_builtin_iter(
        crate::value::BuiltinIterName::Cycle,
        crate::state::BuiltinIterState::Cycle { items, pos: 0 },
    ))
}

/// `starmap(function, iterable)`: call `function(*args)` for each
/// `args` tuple/iterable in the input.
async fn starmap_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let func = need_arg("starmap", args, 0)?.clone();
    let iter_val = need_arg("starmap", args, 1)?.clone();
    let kwargs = indexmap::IndexMap::new();
    let mut out = Vec::new();
    for item in crate::eval::op::iter(state, &iter_val, tools).await? {
        let call_args = iterate_value(&item)?;
        out.push(
            crate::eval::modules::call_callable(state, &func, &call_args, &kwargs, tools).await?,
        );
    }
    Ok(Value::List(shared_list(out)))
}

/// `filterfalse(predicate, iterable)`: keep items where `predicate` is
/// falsy (`predicate=None` keeps falsy items directly).
async fn filterfalse_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let pred = need_arg("filterfalse", args, 0)?.clone();
    let iter_val = need_arg("filterfalse", args, 1)?.clone();
    let kwargs = indexmap::IndexMap::new();
    let mut out = Vec::new();
    for item in crate::eval::op::iter(state, &iter_val, tools).await? {
        let keep = if matches!(pred, Value::None) {
            !item.is_truthy()
        } else {
            !crate::eval::modules::call_callable(
                state,
                &pred,
                std::slice::from_ref(&item),
                &kwargs,
                tools,
            )
            .await?
            .is_truthy()
        };
        if keep {
            out.push(item);
        }
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
            "islice" => islice_impl(state, args, tools).await,
            "starmap" => starmap_impl(state, args, tools).await,
            "filterfalse" => filterfalse_impl(state, args, tools).await,
            "count" => make_count(state, args),
            "cycle" => make_cycle(state, args),
            // `repeat(obj)` with no count is the unbounded form (the
            // counted form stays eager in the sync `call`).
            "repeat" if args.len() < 2 => Ok(state.alloc_builtin_iter(
                crate::value::BuiltinIterName::Repeat,
                crate::state::BuiltinIterState::Repeat {
                    value: need_arg("repeat", args, 0)?.clone(),
                },
            )),
            _ => call(func, args, kwargs),
        }
    }
}
