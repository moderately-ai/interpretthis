// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `list` method dispatch — mutating + read-only methods that share
//! the in-place receiver model (`MethodOutcome::grew`/`shrank` returns
//! how the caller's memory tracker should adjust).
//!
//! `list.sort(key=)` is intercepted at `eval_call` because it needs
//! async key= dispatch. Most list methods are positional-only in
//! CPython 3.12 — unexpected kwargs raise TypeError.

use indexmap::IndexMap;

use super::super::{
    MethodOutcome, arg1, reject_kwargs, sequence_index_range, to_index, to_len_i64, value_to_i64,
};
use crate::{
    error::{EvalError, InterpreterError},
    eval::control_flow::iterate_value,
    state::estimate_value_size,
    value::{ExceptionValue, Value, shared_list},
};

pub(crate) fn dispatch_list_method(
    items: &mut Vec<Value>,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    use crate::eval::operations::values_equal_pub;

    // CPython 3.12 list methods are positional-only (except sort, handled
    // in eval_call). Reject kwargs so they are never silently dropped.
    reject_kwargs(method, kwargs)?;

    match method {
        // `list.copy()` is a shallow copy: new SharedList, same inner elements.
        "copy" => Ok(MethodOutcome::pure(Value::List(shared_list(items.clone())))),
        "count" => {
            if args.len() != 1 {
                return Err(InterpreterError::TypeError(
                    "count() takes exactly one argument".into(),
                )
                .into());
            }
            let count = items.iter().filter(|v| values_equal_pub(v, &args[0])).count();
            Ok(MethodOutcome::pure(Value::Int(to_len_i64(count)?)))
        }
        "index" => {
            let target = arg1(method, args)?;
            let (start, end) = sequence_index_range(method, args, items.len())?;
            for (i, item) in items.iter().enumerate().take(end).skip(start) {
                if values_equal_pub(item, target) {
                    return Ok(MethodOutcome::pure(Value::Int(to_len_i64(i)?)));
                }
            }
            Err(EvalError::Exception(ExceptionValue::new(
                "ValueError",
                format!("{} is not in list", target.repr()),
            )))
        }
        "append" => {
            let arg = arg1(method, args)?;
            let size = estimate_value_size(arg);
            items.push(arg.clone());
            Ok(MethodOutcome::grew(Value::None, size))
        }
        "extend" => {
            let new_items = iterate_value(arg1(method, args)?)?;
            let added: usize = new_items.iter().map(estimate_value_size).sum();
            items.extend(new_items);
            Ok(MethodOutcome::grew(Value::None, added))
        }
        "insert" => {
            if args.len() != 2 {
                return Err(InterpreterError::TypeError(
                    "insert() takes exactly 2 arguments".into(),
                )
                .into());
            }
            let idx = value_to_i64(&args[0])?;
            let size = estimate_value_size(&args[1]);
            let len = to_len_i64(items.len())?;
            // Negative indices saturate at the front; positive ones clamp to the
            // end, matching CPython's `list.insert` out-of-range behaviour.
            let pos = if idx < 0 {
                to_index((len + idx).max(0))?
            } else {
                to_index(idx)?.min(items.len())
            };
            items.insert(pos, args[1].clone());
            Ok(MethodOutcome::grew(Value::None, size))
        }
        "pop" => {
            if items.is_empty() {
                return Err(EvalError::Exception(ExceptionValue::new(
                    "IndexError",
                    "pop from empty list",
                )));
            }
            let i = match args.first() {
                None => items.len() - 1,
                Some(arg) => {
                    let raw = value_to_i64(arg)?;
                    let len = to_len_i64(items.len())?;
                    let normalized = if raw < 0 { len + raw } else { raw };
                    if normalized < 0 || normalized >= len {
                        return Err(EvalError::Exception(ExceptionValue::index_error("pop")));
                    }
                    to_index(normalized)?
                }
            };
            let val = items.remove(i);
            let freed = estimate_value_size(&val);
            Ok(MethodOutcome::shrank(val, freed))
        }
        "remove" => {
            let target = arg1(method, args)?;
            let Some(idx) = items.iter().position(|v| values_equal_pub(v, target)) else {
                return Err(EvalError::Exception(ExceptionValue::new(
                    "ValueError",
                    "list.remove(x): x not in list",
                )));
            };
            let removed = items.remove(idx);
            Ok(MethodOutcome::shrank(Value::None, estimate_value_size(&removed)))
        }
        // `list.sort()` is intercepted at `eval_call` because it takes
        // keyword-only args (`key=`, `reverse=`) and needs async key=.
        "reverse" => {
            items.reverse();
            Ok(MethodOutcome::pure(Value::None))
        }
        "clear" => {
            let freed: usize = items.iter().map(estimate_value_size).sum();
            items.clear();
            Ok(MethodOutcome::shrank(Value::None, freed))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'list' object has no attribute '{method}'"
        ))
        .into()),
    }
}
