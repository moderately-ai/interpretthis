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
) -> Result<MethodOutcome, EvalError> {
    match method {
        // Inherited from dict — same shape, same outcome.
        "keys" | "values" | "items" | "get" | "copy" | "pop" | "setdefault" | "clear" => {
            let outcome = dict::dispatch_dict_method(map, method, args)?;
            // `copy` of a Counter returns a Counter, not a dict — fix
            // up the return value here so callers see the right type.
            let value = if method == "copy" {
                match outcome.value {
                    Value::Dict(d) => Value::Counter(d),
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
                Some(Value::Int(i)) => usize::try_from(*i).unwrap_or(entries.len()),
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
        // `c.subtract(other)` decrements counts. Unlike +/- which
        // keep_positive, subtract allows zero and negative counts.
        "subtract" => {
            counter_apply_in_place(map, args.first(), |cur, delta| cur - delta)?;
            Ok(MethodOutcome::pure(Value::None))
        }
        // `c.update(other)` increments counts (DIFFERS from dict.update
        // which overwrites). Same semantics as Counter(iter) on first
        // construction.
        "update" => {
            counter_apply_in_place(map, args.first(), |cur, delta| cur + delta)?;
            Ok(MethodOutcome::pure(Value::None))
        }
        // `c.total()` returns the sum of all counts.
        "total" => {
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

/// Apply `op` to each (key, delta) from `other_arg` against `map`'s
/// current entries. Used by Counter's subtract and update methods.
/// CPython accepts a mapping or an iterable; we accept Dict / Counter
/// for the mapping case and iterate for everything else (tallying
/// each item once before applying).
fn counter_apply_in_place(
    map: &mut IndexMap<ValueKey, Value>,
    other_arg: Option<&Value>,
    op: fn(i64, i64) -> i64,
) -> Result<(), EvalError> {
    let Some(other) = other_arg else { return Ok(()) };
    // Mapping branch.
    if let Value::Dict(other_map) | Value::Counter(other_map) = other {
        for (k, v) in other_map {
            let delta = match v {
                Value::Int(i) => *i,
                Value::Bool(b) => i64::from(*b),
                _ => 0,
            };
            let cur = match map.get(k) {
                Some(Value::Int(i)) => *i,
                Some(Value::Bool(b)) => i64::from(*b),
                _ => 0,
            };
            let new_val = op(cur, delta);
            map.insert(k.clone(), Value::Int(new_val));
        }
        return Ok(());
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
        let new_val = op(cur, 1);
        map.insert(key, Value::Int(new_val));
    }
    Ok(())
}
