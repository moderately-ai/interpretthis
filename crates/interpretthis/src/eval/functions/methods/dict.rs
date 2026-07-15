// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `dict` method dispatch — keys/values/items, get, pop, copy,
//! setdefault, update, clear, `OrderedDict.move_to_end` (which we
//! surface on every dict — see CONFORMANCE.md#ordereddict-on-dict).

use indexmap::IndexMap;

use super::super::{MethodOutcome, arg1, bind_method_params, reject_kwargs, require_param};
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
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    match method {
        "keys" => {
            reject_kwargs(method, kwargs)?;
            Ok(MethodOutcome::pure(Value::List(shared_list(
                map.keys().map(ValueKey::to_value).collect(),
            ))))
        }
        "values" => {
            reject_kwargs(method, kwargs)?;
            Ok(MethodOutcome::pure(Value::List(shared_list(map.values().cloned().collect()))))
        }
        "items" => {
            reject_kwargs(method, kwargs)?;
            Ok(MethodOutcome::pure(Value::List(shared_list(
                map.iter().map(|(k, v)| Value::Tuple(vec![k.to_value(), v.clone()])).collect(),
            ))))
        }
        "get" => {
            // CPython: dict.get(self, key, default=None, /) — positional-only.
            reject_kwargs(method, kwargs)?;
            let key = value_to_key(arg1(method, args)?)?;
            let default = args.get(1).cloned().unwrap_or(Value::None);
            Ok(MethodOutcome::pure(map.get(&key).cloned().unwrap_or(default)))
        }
        "copy" => {
            reject_kwargs(method, kwargs)?;
            Ok(MethodOutcome::pure(Value::Dict(crate::value::shared_dict(map.clone()))))
        }
        "pop" => {
            // CPython: dict.pop(self, key, default=<unspecified>, /) — positional-only.
            reject_kwargs(method, kwargs)?;
            let key = value_to_key(arg1(method, args)?)?;
            // shift_remove (not swap_remove) preserves the insertion order of
            // the remaining entries, as CPython's dict.pop does.
            if let Some(val) = map.shift_remove(&key) {
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
            // CPython: update([other], **kwargs). `other` may be a mapping or
            // omitted; kwargs always merge last (string keys).
            if args.len() > 1 {
                return Err(InterpreterError::TypeError(
                    "update() takes at most 1 positional argument".into(),
                )
                .into());
            }
            let mut delta = 0isize;
            if let Some(arg) = args.first() {
                let Value::Dict(new_entries) = arg else {
                    return Err(InterpreterError::TypeError(
                        "dict.update() argument must be a dict".into(),
                    )
                    .into());
                };
                for (k, v) in new_entries.lock().clone() {
                    delta = delta.saturating_add(insert_entry(map, k, v));
                }
            }
            for (k, v) in kwargs {
                let key = ValueKey::String(k.clone().into());
                delta = delta.saturating_add(insert_entry(map, key, v.clone()));
            }
            Ok(MethodOutcome { value: Value::None, mem_delta: delta })
        }
        "setdefault" => {
            // CPython: dict.setdefault(self, key, default=None, /) — positional-only.
            reject_kwargs(method, kwargs)?;
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
            reject_kwargs(method, kwargs)?;
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
            // CPython OrderedDict.move_to_end accepts key=/last= kwargs.
            let bound = bind_method_params(method, args, kwargs, &["key", "last"])?;
            let key = value_to_key(require_param(method, &bound, 0, "key")?)?;
            let last = bound[1].as_ref().is_none_or(Value::is_truthy);
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
        "popitem" => {
            // CPython 3.7+: remove and return the LAST inserted (key, value)
            // pair (LIFO); empty dict raises KeyError.
            reject_kwargs(method, kwargs)?;
            if !args.is_empty() {
                return Err(
                    InterpreterError::TypeError("popitem() takes no arguments".into()).into()
                );
            }
            let Some((key, val)) = map.pop() else {
                return Err(EvalError::Exception(ExceptionValue::new(
                    "KeyError",
                    "'popitem(): dictionary is empty'",
                )));
            };
            let freed = estimate_key_size(&key) + estimate_value_size(&val);
            let pair = Value::Tuple(vec![key.to_value(), val]);
            Ok(MethodOutcome::shrank(pair, freed))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'dict' object has no attribute '{method}'"
        ))
        .into()),
    }
}

fn insert_entry(map: &mut IndexMap<ValueKey, Value>, k: ValueKey, v: Value) -> isize {
    let v_size = estimate_value_size(&v);
    map.insert(k.clone(), v).map_or_else(
        || place::to_isize(estimate_key_size(&k) + v_size),
        |old| place::size_delta(estimate_value_size(&old), v_size),
    )
}
