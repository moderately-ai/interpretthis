// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `set` method dispatch — union/intersection/difference/issubset/
//! issuperset/isdisjoint plus mutating add/remove/discard/pop/clear/
//! update. Sets are stored as `Vec<Value>` (Value is not `Hash`), so
//! membership is a linear scan keyed on `value_to_key`.

use super::super::{MethodOutcome, arg1};
use crate::{
    error::{EvalError, InterpreterError},
    eval::{control_flow::iterate_value, literals::value_to_key},
    state::estimate_value_size,
    value::{ExceptionValue, Value},
};

pub(crate) fn dispatch_set_method(
    items: &mut Vec<Value>,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    // Sets are stored as a `Vec` (Value is not `Hash`), so membership is a
    // linear scan keyed on `value_to_key`.
    let contains = |items: &[Value], probe: &Value| {
        let key = value_to_key(probe).ok();
        items.iter().any(|r| value_to_key(r).ok() == key)
    };

    match method {
        "copy" => Ok(MethodOutcome::pure(Value::Set(items.clone()))),
        "union" => {
            let mut result = items.clone();
            if let Some(arg) = args.first() {
                for item in iterate_value(arg)? {
                    if !contains(&result, &item) {
                        result.push(item);
                    }
                }
            }
            Ok(MethodOutcome::pure(Value::Set(result)))
        }
        "intersection" => {
            let Some(arg) = args.first() else {
                return Ok(MethodOutcome::pure(Value::Set(items.clone())));
            };
            let other = iterate_value(arg)?;
            let other_keys: Vec<_> = other.iter().filter_map(|v| value_to_key(v).ok()).collect();
            let result: Vec<Value> = items
                .iter()
                .filter(|v| value_to_key(v).is_ok_and(|k| other_keys.contains(&k)))
                .cloned()
                .collect();
            Ok(MethodOutcome::pure(Value::Set(result)))
        }
        "difference" => {
            let Some(arg) = args.first() else {
                return Ok(MethodOutcome::pure(Value::Set(items.clone())));
            };
            let other = iterate_value(arg)?;
            let other_keys: Vec<_> = other.iter().filter_map(|v| value_to_key(v).ok()).collect();
            let result: Vec<Value> = items
                .iter()
                .filter(|v| value_to_key(v).map_or(true, |k| !other_keys.contains(&k)))
                .cloned()
                .collect();
            Ok(MethodOutcome::pure(Value::Set(result)))
        }
        "issubset" => {
            let other = iterate_value(arg1(method, args)?)?;
            let other_keys: Vec<_> = other.iter().filter_map(|v| value_to_key(v).ok()).collect();
            let result =
                items.iter().all(|v| value_to_key(v).is_ok_and(|k| other_keys.contains(&k)));
            Ok(MethodOutcome::pure(Value::Bool(result)))
        }
        "issuperset" => {
            let other = iterate_value(arg1(method, args)?)?;
            let self_keys: Vec<_> = items.iter().filter_map(|v| value_to_key(v).ok()).collect();
            let result =
                other.iter().all(|v| value_to_key(v).is_ok_and(|k| self_keys.contains(&k)));
            Ok(MethodOutcome::pure(Value::Bool(result)))
        }
        "isdisjoint" => {
            let other = iterate_value(arg1(method, args)?)?;
            let other_keys: Vec<_> = other.iter().filter_map(|v| value_to_key(v).ok()).collect();
            let result =
                !items.iter().any(|v| value_to_key(v).is_ok_and(|k| other_keys.contains(&k)));
            Ok(MethodOutcome::pure(Value::Bool(result)))
        }
        "add" => {
            let arg = arg1(method, args)?;
            // A genuinely-unhashable element (list/dict/set) raises. Instances
            // are hashable by identity in CPython, so they are allowed (their
            // structural dedup here is best-effort — sets are stored as a Vec
            // and these methods are sync, so async `__eq__` cannot run).
            if !matches!(arg, Value::Instance(_)) {
                value_to_key(arg)?;
            }
            if contains(items, arg) {
                Ok(MethodOutcome::pure(Value::None))
            } else {
                let size = estimate_value_size(arg);
                items.push(arg.clone());
                Ok(MethodOutcome::grew(Value::None, size))
            }
        }
        "remove" => {
            let arg = arg1(method, args)?;
            let key = value_to_key(arg).ok();
            let Some(idx) = items.iter().position(|r| value_to_key(r).ok() == key) else {
                return Err(EvalError::Exception(ExceptionValue::new("KeyError", arg.repr())));
            };
            let removed = items.remove(idx);
            Ok(MethodOutcome::shrank(Value::None, estimate_value_size(&removed)))
        }
        "discard" => {
            let arg = arg1(method, args)?;
            let key = value_to_key(arg).ok();
            // discard() on a missing element is a no-op.
            let Some(idx) = items.iter().position(|r| value_to_key(r).ok() == key) else {
                return Ok(MethodOutcome::pure(Value::None));
            };
            let removed = items.remove(idx);
            Ok(MethodOutcome::shrank(Value::None, estimate_value_size(&removed)))
        }
        "pop" => {
            if items.is_empty() {
                return Err(EvalError::Exception(ExceptionValue::new(
                    "KeyError",
                    "pop from an empty set",
                )));
            }
            let val = items.remove(0);
            let freed = estimate_value_size(&val);
            Ok(MethodOutcome::shrank(val, freed))
        }
        "clear" => {
            let freed: usize = items.iter().map(estimate_value_size).sum();
            items.clear();
            Ok(MethodOutcome::shrank(Value::None, freed))
        }
        "symmetric_difference" => {
            let other = iterate_value(arg1(method, args)?)?;
            let self_keys: Vec<_> = items.iter().filter_map(|v| value_to_key(v).ok()).collect();
            let other_keys: Vec<_> = other.iter().filter_map(|v| value_to_key(v).ok()).collect();
            let mut result: Vec<Value> = items
                .iter()
                .filter(|v| value_to_key(v).map_or(true, |k| !other_keys.contains(&k)))
                .cloned()
                .collect();
            for item in other {
                if value_to_key(&item).is_ok_and(|k| !self_keys.contains(&k))
                    && !contains(&result, &item)
                {
                    result.push(item);
                }
            }
            Ok(MethodOutcome::pure(Value::Set(result)))
        }
        "update" => {
            let Some(arg) = args.first() else { return Ok(MethodOutcome::pure(Value::None)) };
            let new_items = iterate_value(arg)?;
            let mut added = 0usize;
            for item in new_items {
                if !contains(items, &item) {
                    added += estimate_value_size(&item);
                    items.push(item);
                }
            }
            Ok(MethodOutcome::grew(Value::None, added))
        }
        "intersection_update" => {
            let other = iterate_value(arg1(method, args)?)?;
            let other_keys: Vec<_> = other.iter().filter_map(|v| value_to_key(v).ok()).collect();
            items.retain(|v| value_to_key(v).is_ok_and(|k| other_keys.contains(&k)));
            Ok(MethodOutcome::pure(Value::None))
        }
        "difference_update" => {
            let other = iterate_value(arg1(method, args)?)?;
            let other_keys: Vec<_> = other.iter().filter_map(|v| value_to_key(v).ok()).collect();
            items.retain(|v| value_to_key(v).map_or(true, |k| !other_keys.contains(&k)));
            Ok(MethodOutcome::pure(Value::None))
        }
        "symmetric_difference_update" => {
            let other = iterate_value(arg1(method, args)?)?;
            let self_keys: Vec<_> = items.iter().filter_map(|v| value_to_key(v).ok()).collect();
            // Drop shared elements, then append the other side's uniques.
            let other_keys: Vec<_> = other.iter().filter_map(|v| value_to_key(v).ok()).collect();
            items.retain(|v| value_to_key(v).map_or(true, |k| !other_keys.contains(&k)));
            let mut added = 0usize;
            for item in other {
                if value_to_key(&item).is_ok_and(|k| !self_keys.contains(&k))
                    && !contains(items, &item)
                {
                    added += estimate_value_size(&item);
                    items.push(item);
                }
            }
            Ok(MethodOutcome::grew(Value::None, added))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'set' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Non-mutating `frozenset` methods — the set-algebra subset that returns a
/// new value. Delegates to [`dispatch_set_method`] on a copy (so no mutation
/// escapes) and rewraps any `set` result as a `frozenset`. Mutating method
/// names raise `AttributeError`, matching CPython's immutable `frozenset`.
pub(crate) fn dispatch_frozenset_method(
    items: &[Value],
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    const FROZENSET_METHODS: &[&str] = &[
        "copy",
        "union",
        "intersection",
        "difference",
        "symmetric_difference",
        "issubset",
        "issuperset",
        "isdisjoint",
    ];
    if !FROZENSET_METHODS.contains(&method) {
        return Err(InterpreterError::AttributeError(format!(
            "'frozenset' object has no attribute '{method}'"
        ))
        .into());
    }
    let mut scratch = items.to_vec();
    let outcome = dispatch_set_method(&mut scratch, method, args, kwargs)?;
    // The delegated methods above are all non-mutating, so `scratch` is
    // untouched and only the returned value matters; a set result becomes a
    // frozenset, a bool stays a bool.
    let value = match outcome.value {
        Value::Set(v) => Value::Frozenset(v),
        other => other,
    };
    Ok(MethodOutcome::pure(value))
}
