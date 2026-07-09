// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `dict` method dispatch — keys/values/items, get, pop, copy,
//! setdefault, update, clear, `OrderedDict.move_to_end` (which we
//! surface on every dict — see CONFORMANCE.md#ordereddict-on-dict).

use indexmap::IndexMap;

use super::super::{MethodOutcome, arg1};
use crate::{
    error::{EvalError, InterpreterError},
    eval::{literals::value_to_key, place},
    state::{estimate_key_size, estimate_value_size},
    value::{ExceptionValue, Value, ValueKey, shared_list},
};

pub(crate) fn dispatch_dict_method(
    map: &mut IndexMap<ValueKey, Value>,
    method: &str,
    args: &[Value],
) -> Result<MethodOutcome, EvalError> {
    match method {
        "keys" => Ok(MethodOutcome::pure(Value::List(shared_list(
            map.keys().map(ValueKey::to_value).collect(),
        )))),
        "values" => {
            Ok(MethodOutcome::pure(Value::List(shared_list(map.values().cloned().collect()))))
        }
        "items" => Ok(MethodOutcome::pure(Value::List(shared_list(
            map.iter().map(|(k, v)| Value::Tuple(vec![k.to_value(), v.clone()])).collect(),
        )))),
        "get" => {
            let key = value_to_key(arg1(method, args)?)?;
            let default = args.get(1).cloned().unwrap_or(Value::None);
            Ok(MethodOutcome::pure(map.get(&key).cloned().unwrap_or(default)))
        }
        "copy" => Ok(MethodOutcome::pure(Value::Dict(map.clone()))),
        "pop" => {
            let key = value_to_key(arg1(method, args)?)?;
            if let Some(val) = map.swap_remove(&key) {
                let freed = estimate_key_size(&key) + estimate_value_size(&val);
                return Ok(MethodOutcome::shrank(val, freed));
            }
            // Missing key: a `default` arg is returned, else it's a KeyError.
            if let Some(def) = args.get(1) {
                return Ok(MethodOutcome::pure(def.clone()));
            }
            Err(EvalError::Exception(ExceptionValue::key_error(key)))
        }
        "update" => {
            let Some(arg) = args.first() else { return Ok(MethodOutcome::pure(Value::None)) };
            let Value::Dict(new_entries) = arg else {
                return Err(InterpreterError::TypeError(
                    "dict.update() argument must be a dict".into(),
                )
                .into());
            };
            // Net delta: an overwrite changes only the value's size; a new key
            // adds key + value. Computed precisely so the budget stays accurate.
            let mut delta = 0isize;
            for (k, v) in new_entries.clone() {
                let v_size = estimate_value_size(&v);
                let entry_delta = map.insert(k.clone(), v).map_or_else(
                    || place::to_isize(estimate_key_size(&k) + v_size),
                    |old| place::size_delta(estimate_value_size(&old), v_size),
                );
                delta = delta.saturating_add(entry_delta);
            }
            Ok(MethodOutcome { value: Value::None, mem_delta: delta })
        }
        "setdefault" => {
            let key = value_to_key(arg1(method, args)?)?;
            let default = args.get(1).cloned().unwrap_or(Value::None);
            if let Some(existing) = map.get(&key) {
                return Ok(MethodOutcome::pure(existing.clone()));
            }
            let entry_size = estimate_key_size(&key) + estimate_value_size(&default);
            let returned = default.clone();
            map.insert(key, default);
            Ok(MethodOutcome::grew(returned, entry_size))
        }
        "clear" => {
            let freed: usize =
                map.iter().map(|(k, v)| estimate_key_size(k) + estimate_value_size(v)).sum();
            map.clear();
            Ok(MethodOutcome::shrank(Value::None, freed))
        }
        // `OrderedDict.move_to_end(key, last=True)` — relocate the
        // entry to the back (last=True, default) or the front
        // (last=False). Since we model OrderedDict as a regular Dict
        // (CPython's dict has been insertion-ordered since 3.7), this
        // surfaces on every Dict; that's a documented divergence —
        // CPython's plain dict raises AttributeError for
        // `.move_to_end`. See CONFORMANCE.md#ordereddict-on-dict.
        "move_to_end" => {
            let key = value_to_key(arg1(method, args)?)?;
            let last = args.get(1).is_none_or(Value::is_truthy);
            let Some((existing_key, val)) = map.shift_remove_entry(&key) else {
                return Err(EvalError::Exception(ExceptionValue::key_error(key)));
            };
            if last {
                map.insert(existing_key, val);
            } else {
                map.shift_insert(0, existing_key, val);
            }
            Ok(MethodOutcome::pure(Value::None))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'dict' object has no attribute '{method}'"
        ))
        .into()),
    }
}
