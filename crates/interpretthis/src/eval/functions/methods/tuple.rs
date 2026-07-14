// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `tuple` method dispatch — `count(x)` and `index(x)`. Tuples are
//! immutable so the surface is read-only.

use super::super::to_len_i64;
use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    value::{ExceptionValue, Value},
};

pub(crate) fn dispatch_tuple_method(
    items: &[Value],
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "count" => {
            if args.len() != 1 {
                return Err(
                    InterpreterError::TypeError("count() takes exactly 1 argument".into()).into()
                );
            }
            let count = items
                .iter()
                .filter(|v| crate::eval::operations::values_equal_pub(v, &args[0]))
                .count();
            Ok(Value::Int(to_len_i64(count)?))
        }
        "index" => {
            let target = args.first().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "index() takes at least 1 argument".into(),
                ))
            })?;
            let (start, end) =
                crate::eval::functions::sequence_index_range(method, args, items.len())?;
            for (i, item) in items.iter().enumerate().take(end).skip(start) {
                if crate::eval::operations::values_equal_pub(item, target) {
                    return Ok(Value::Int(to_len_i64(i)?));
                }
            }
            Err(EvalError::Exception(ExceptionValue::new(
                "ValueError",
                "tuple.index(x): x not in tuple",
            )))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'tuple' object has no attribute '{method}'"
        ))
        .into()),
    }
}
