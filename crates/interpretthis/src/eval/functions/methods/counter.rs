// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `collections.Counter` method dispatch — inherits dict's read
//! surface plus Counter-specific most_common/elements/subtract/
//! update/total. The shared dict methods route through
//! `methods::dict::dispatch_dict_method` against the same backing map;
//! `copy` is post-processed so it returns a `Value::Counter` rather
//! than a `Value::Dict`.

use indexmap::IndexMap;

use super::{super::MethodOutcome, dict};
use crate::{
    error::{EvalError, InterpreterError},
    eval::{control_flow::iterate_value, literals::value_to_key},
    value::{Value, ValueKey, shared_list},
};

pub(crate) fn dispatch_counter_method(
    map: &mut IndexMap<ValueKey, Value>,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    match method {
        // Inherited from dict — same shape, same outcome.
        "keys" | "values" | "items" | "get" | "copy" | "pop" | "setdefault" | "clear" => {
            let outcome = dict::dispatch_dict_method(map, method, args, kwargs)?;
            // `copy` of a Counter returns a Counter, not a dict — fix
            // up the return value here so callers see the right type.
            let value = if method == "copy" {
                match outcome.value {
                    Value::Dict(d) => Value::Counter(d.lock().clone()),
                    other => other,
                }
            } else {
                outcome.value
            };
            Ok(MethodOutcome { value, mem_delta: outcome.mem_delta })
        }
        // `c.most_common(n)` returns a list of (key, count) tuples
        // sorted by count descending (stable for ties). `n=None` or
        // absent returns all entries.
        "most_common" => {
            crate::eval::functions::reject_kwargs(method, kwargs)?;
            let mut entries: Vec<(ValueKey, i64)> = map
                .iter()
                .map(|(k, v)| {
                    let n = match v {
                        Value::Int(i) => *i,
                        Value::Bool(b) => i64::from(*b),
                        _ => 0,
                    };
                    (k.clone(), n)
                })
                .collect();
            // Sort by count DESC, preserving first-appearance order for
            // ties (CPython uses a stable sort on -count).
            entries.sort_by_key(|entry| std::cmp::Reverse(entry.1));
            let n = match args.first() {
                // A negative n is CPython's "empty result"; a huge positive n
                // that overflows usize just means "all entries" (take clamps).
                Some(Value::Int(i)) => usize::try_from((*i).max(0)).unwrap_or(entries.len()),
                Some(Value::Bool(b)) => usize::from(*b),
                None | Some(Value::None) => entries.len(),
                Some(_) => {
                    return Err(InterpreterError::TypeError(
                        "most_common(): n must be an integer or None".into(),
                    )
                    .into());
                }
            };
            let result: Vec<Value> = entries
                .into_iter()
                .take(n)
                .map(|(k, n)| Value::Tuple(vec![k.to_value(), Value::Int(n)]))
                .collect();
            Ok(MethodOutcome::pure(Value::List(shared_list(result))))
        }
        // `c.elements()` yields each key `count` times. CPython returns
        // an iterator; we materialise a list (consistent with the rest
        // of the interpreter's eager iteration model).
        "elements" => {
            crate::eval::functions::reject_kwargs(method, kwargs)?;
            let mut out: Vec<Value> = Vec::new();
            for (key, value) in map.iter() {
                let n = match value {
                    Value::Int(i) => *i,
                    Value::Bool(b) => i64::from(*b),
                    _ => 0,
                };
                if n > 0 {
                    let key_val = key.to_value();
                    for _ in 0..n {
                        out.push(key_val.clone());
                    }
                }
            }
            Ok(MethodOutcome::pure(Value::List(shared_list(out))))
        }
        // `c.subtract(other, **kwargs)` decrements counts. Unlike +/- which
        // keep_positive, subtract allows zero and negative counts.
        "subtract" => {
            let mut delta = counter_apply_in_place(map, args.first(), |cur, delta| cur - delta)?;
            delta += counter_apply_kwargs(map, kwargs, |cur, delta| cur - delta);
            Ok(counter_mem_outcome(delta))
        }
        // `c.update(other, **kwargs)` increments counts (DIFFERS from
        // dict.update which overwrites). Same semantics as Counter(iter) on
        // first construction; keyword counts merge last (`c.update(a=1)`).
        "update" => {
            let mut delta = counter_apply_in_place(map, args.first(), |cur, delta| cur + delta)?;
            delta += counter_apply_kwargs(map, kwargs, |cur, delta| cur + delta);
            Ok(counter_mem_outcome(delta))
        }
        // `c.total()` returns the sum of all counts.
        "total" => {
            crate::eval::functions::reject_kwargs(method, kwargs)?;
            let total: i64 = map
                .values()
                .map(|v| match v {
                    Value::Int(i) => *i,
                    Value::Bool(b) => i64::from(*b),
                    _ => 0,
                })
                .sum();
            Ok(MethodOutcome::pure(Value::Int(total)))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'Counter' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Build the `MethodOutcome` carrying the accumulated byte delta so the caller
/// charges the new entries against the memory budget (Counter growth was
/// previously unaccounted — a `c.update([...new keys...])` loop could evade the
/// memory limit).
fn counter_mem_outcome(mem_delta: isize) -> MethodOutcome {
    MethodOutcome { value: Value::None, mem_delta }
}

/// Apply keyword counts (`c.update(a=1, b=2)`) against `map` with `op`.
/// Each keyword is a string key whose value is the integer delta. Returns the
/// byte delta of newly-added entries.
fn counter_apply_kwargs(
    map: &mut IndexMap<ValueKey, Value>,
    kwargs: &IndexMap<String, Value>,
    op: fn(i64, i64) -> i64,
) -> isize {
    let mut mem_delta = 0;
    for (name, value) in kwargs {
        let amount = match value {
            Value::Int(i) => *i,
            Value::Bool(b) => i64::from(*b),
            _ => continue,
        };
        let key = ValueKey::String(name.as_str().into());
        let cur = match map.get(&key) {
            Some(Value::Int(i)) => *i,
            Some(Value::Bool(b)) => i64::from(*b),
            _ => 0,
        };
        mem_delta += crate::eval::functions::methods::dict::insert_entry(
            map,
            key,
            Value::Int(op(cur, amount)),
        );
    }
    mem_delta
}

/// Apply `op` to each (key, delta) from `other_arg` against `map`'s
/// current entries. Used by Counter's subtract and update methods.
/// CPython accepts a mapping or an iterable; we accept Dict / Counter
/// for the mapping case and iterate for everything else (tallying
/// each item once before applying).
fn counter_apply_in_place(
    map: &mut IndexMap<ValueKey, Value>,
    other_arg: Option<&Value>,
    op: fn(i64, i64) -> i64,
) -> Result<isize, EvalError> {
    let Some(other) = other_arg else { return Ok(0) };
    let insert_entry = crate::eval::functions::methods::dict::insert_entry;
    let mut mem_delta = 0;
    // Mapping branch. Snapshot Dict/Counter contents into an owned map.
    let other_map = match other {
        Value::Dict(m) => Some(m.lock().clone()),
        Value::Counter(m) => Some(m.clone()),
        _ => None,
    };
    if let Some(other_map) = other_map {
        for (k, v) in &other_map {
            let amount = match v {
                Value::Int(i) => *i,
                Value::Bool(b) => i64::from(*b),
                _ => 0,
            };
            let cur = match map.get(k) {
                Some(Value::Int(i)) => *i,
                Some(Value::Bool(b)) => i64::from(*b),
                _ => 0,
            };
            mem_delta += insert_entry(map, k.clone(), Value::Int(op(cur, amount)));
        }
        return Ok(mem_delta);
    }
    // Iterable branch: each item adds 1 to its slot. For subtract,
    // each item subtracts 1.
    for item in iterate_value(other)? {
        let key = value_to_key(&item)?;
        let cur = match map.get(&key) {
            Some(Value::Int(i)) => *i,
            Some(Value::Bool(b)) => i64::from(*b),
            _ => 0,
        };
        mem_delta += insert_entry(map, key, Value::Int(op(cur, 1)));
    }
    Ok(mem_delta)
}
