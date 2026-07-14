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
    // linear scan under the shared structural comparator. `value_to_key`
    // returns `None` for every instance, so keying on it would collapse
    // distinct objects into one — `set_contains` distinguishes them.
    use crate::eval::operations::{set_contains, values_equal_pub};
    let position =
        |items: &[Value], probe: &Value| items.iter().position(|r| values_equal_pub(r, probe));

    match method {
        "copy" => Ok(MethodOutcome::pure(Value::Set(items.clone()))),
        "union" => {
            let mut result = items.clone();
            if let Some(arg) = args.first() {
                for item in iterate_value(arg)? {
                    if !set_contains(&result, &item) {
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
            let result: Vec<Value> =
                items.iter().filter(|v| set_contains(&other, v)).cloned().collect();
            Ok(MethodOutcome::pure(Value::Set(result)))
        }
        "difference" => {
            let Some(arg) = args.first() else {
                return Ok(MethodOutcome::pure(Value::Set(items.clone())));
            };
            let other = iterate_value(arg)?;
            let result: Vec<Value> =
                items.iter().filter(|v| !set_contains(&other, v)).cloned().collect();
            Ok(MethodOutcome::pure(Value::Set(result)))
        }
        "issubset" => {
            let other = iterate_value(arg1(method, args)?)?;
            let result = items.iter().all(|v| set_contains(&other, v));
            Ok(MethodOutcome::pure(Value::Bool(result)))
        }
        "issuperset" => {
            let other = iterate_value(arg1(method, args)?)?;
            let result = other.iter().all(|v| set_contains(items, v));
            Ok(MethodOutcome::pure(Value::Bool(result)))
        }
        "isdisjoint" => {
            let other = iterate_value(arg1(method, args)?)?;
            let result = !items.iter().any(|v| set_contains(&other, v));
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
            if set_contains(items, arg) {
                Ok(MethodOutcome::pure(Value::None))
            } else {
                let size = estimate_value_size(arg);
                items.push(arg.clone());
                Ok(MethodOutcome::grew(Value::None, size))
            }
        }
        "remove" => {
            let arg = arg1(method, args)?;
            let Some(idx) = position(items, arg) else {
                return Err(EvalError::Exception(ExceptionValue::new("KeyError", arg.repr())));
            };
            let removed = items.remove(idx);
            Ok(MethodOutcome::shrank(Value::None, estimate_value_size(&removed)))
        }
        "discard" => {
            let arg = arg1(method, args)?;
            // discard() on a missing element is a no-op.
            let Some(idx) = position(items, arg) else {
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
            let mut result: Vec<Value> =
                items.iter().filter(|v| !set_contains(&other, v)).cloned().collect();
            for item in other {
                if !set_contains(items, &item) && !set_contains(&result, &item) {
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
                if !set_contains(items, &item) {
                    added += estimate_value_size(&item);
                    items.push(item);
                }
            }
            Ok(MethodOutcome::grew(Value::None, added))
        }
        "intersection_update" => {
            let other = iterate_value(arg1(method, args)?)?;
            items.retain(|v| set_contains(&other, v));
            Ok(MethodOutcome::pure(Value::None))
        }
        "difference_update" => {
            let other = iterate_value(arg1(method, args)?)?;
            items.retain(|v| !set_contains(&other, v));
            Ok(MethodOutcome::pure(Value::None))
        }
        "symmetric_difference_update" => {
            let other = iterate_value(arg1(method, args)?)?;
            // Snapshot original membership before mutating, so the decision of
            // which `other` items to append is made against the pre-image.
            let original = items.clone();
            items.retain(|v| !set_contains(&other, v));
            let mut added = 0usize;
            for item in other {
                if !set_contains(&original, &item) && !set_contains(items, &item) {
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
