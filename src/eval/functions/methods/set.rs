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
        _ => Err(InterpreterError::AttributeError(format!(
            "'set' object has no attribute '{method}'"
        ))
        .into()),
    }
}
