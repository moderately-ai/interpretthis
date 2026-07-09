// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `int` method dispatch — wires the commonly-used CPython methods on
//! integer receivers (`bit_length`, `bit_count`, `conjugate`, `real`,
//! `imag`). See the parent module's `dispatch_method` for the routing
//! hub.

use crate::{
    error::{EvalResult, InterpreterError},
    value::Value,
};

/// Dispatch a method call on an `int` receiver. CPython exposes
/// `bit_length`, `bit_count`, `to_bytes`, `from_bytes`, `as_integer_ratio`,
/// `conjugate`, `real`, `imag` — wire the commonly-used ones.
pub(crate) fn dispatch_int_method(i: i64, method: &str, _args: &[Value]) -> EvalResult {
    match method {
        "bit_length" => {
            // CPython: 0.bit_length() == 0; -42.bit_length() == 6
            // (sign ignored). Implemented as `abs(i).leading_zeros`
            // bookkeeping.
            let n = i.unsigned_abs();
            let bits = if n == 0 { 0 } else { i64::from(u64::BITS - n.leading_zeros()) };
            Ok(Value::Int(bits))
        }
        "bit_count" => {
            // Python 3.10+: returns the number of 1 bits in abs(i).
            let n = i.unsigned_abs();
            Ok(Value::Int(i64::from(n.count_ones())))
        }
        "conjugate" | "real" => Ok(Value::Int(i)),
        "imag" => Ok(Value::Int(0)),
        _ => Err(InterpreterError::AttributeError(format!(
            "'int' object has no attribute '{method}'"
        ))
        .into()),
    }
}
