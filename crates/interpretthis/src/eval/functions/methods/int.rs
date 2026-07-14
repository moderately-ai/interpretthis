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

/// Extract the optional `ndigits` argument of `int.__round__` /
/// `float.__round__`. CPython accepts an int (or `None`) and rejects
/// anything else with TypeError.
fn round_ndigits(args: &[Value]) -> Result<Option<i64>, crate::error::EvalError> {
    match args.first() {
        None | Some(Value::None) => Ok(None),
        Some(Value::Int(n)) => Ok(Some(*n)),
        Some(Value::Bool(b)) => Ok(Some(i64::from(*b))),
        Some(other) => Err(InterpreterError::TypeError(format!(
            "'{}' object cannot be interpreted as an integer",
            other.type_name()
        ))
        .into()),
    }
}

/// Dispatch a method call on an `int` receiver. CPython exposes
/// `bit_length`, `bit_count`, `to_bytes`, `from_bytes`, `as_integer_ratio`,
/// `conjugate`, `real`, `imag` — wire the commonly-used ones.
pub(crate) fn dispatch_int_method(
    i: i64,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "__round__" => Ok(crate::eval::functions::round_int(i, round_ndigits(args)?)),
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
        "conjugate" | "real" | "numerator" => Ok(Value::Int(i)),
        "imag" => Ok(Value::Int(0)),
        "denominator" => Ok(Value::Int(1)),
        // `int.as_integer_ratio()` — every int is `(i, 1)`.
        "as_integer_ratio" => Ok(Value::Tuple(vec![Value::Int(i), Value::Int(1)])),
        // Identity conversions CPython exposes as dunders. `int()` /
        // `index()` / `operator.index` route here for user code that
        // calls them explicitly.
        "__index__" | "__int__" | "__trunc__" | "__floor__" | "__ceil__" | "__pos__" => {
            Ok(Value::Int(i))
        }
        "__float__" =>
        {
            #[allow(clippy::cast_precision_loss)]
            Ok(Value::Float(i as f64))
        }
        "__bool__" => Ok(Value::Bool(i != 0)),
        "__invert__" => Ok(Value::Int(!i)),
        // `checked_*` promote i64::MIN to BigInt rather than wrapping.
        "__neg__" => Ok(i.checked_neg().map_or_else(
            || crate::value::int_from_bigint(-num_bigint::BigInt::from(i)),
            Value::Int,
        )),
        "__abs__" => Ok(i.checked_abs().map_or_else(
            // Only i64::MIN reaches here; its magnitude needs BigInt.
            || crate::value::int_from_bigint(-num_bigint::BigInt::from(i)),
            Value::Int,
        )),
        _ => Err(InterpreterError::AttributeError(format!(
            "'int' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Dispatch a method on a `BigInt` receiver whose magnitude exceeds
/// i64. These mirror [`dispatch_int_method`] but stay in arbitrary
/// precision so `(2**100).bit_length()` / `.__index__()` don't
/// spuriously raise `OverflowError` from an i64 narrowing.
pub(crate) fn dispatch_bigint_method(
    b: &num_bigint::BigInt,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    use num_traits::ToPrimitive;
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    let big = |n: num_bigint::BigInt| Ok(crate::value::int_from_bigint(n));
    match method {
        "__round__" => big(crate::eval::functions::round_bigint(b, round_ndigits(args)?)),
        // `BigInt::bits()` counts magnitude bits — exactly CPython's
        // sign-agnostic `bit_length`.
        "bit_length" => Ok(Value::Int(i64::try_from(b.bits()).unwrap_or(i64::MAX))),
        "bit_count" => {
            let ones: u64 = b.iter_u64_digits().map(u64::count_ones).map(u64::from).sum();
            Ok(Value::Int(i64::try_from(ones).unwrap_or(i64::MAX)))
        }
        "conjugate" | "real" | "numerator" | "__index__" | "__int__" | "__trunc__"
        | "__floor__" | "__ceil__" | "__pos__" => big(b.clone()),
        "imag" => Ok(Value::Int(0)),
        "denominator" => Ok(Value::Int(1)),
        "as_integer_ratio" => {
            Ok(Value::Tuple(vec![crate::value::int_from_bigint(b.clone()), Value::Int(1)]))
        }
        "__neg__" => big(-b.clone()),
        "__abs__" => big(if b.sign() == num_bigint::Sign::Minus { -b.clone() } else { b.clone() }),
        // `~x == -x - 1`.
        "__invert__" => big(-b.clone() - 1),
        "__float__" => Ok(Value::Float(b.to_f64().unwrap_or(f64::INFINITY))),
        "__bool__" => Ok(Value::Bool(!num_traits::Zero::is_zero(b))),
        _ => Err(InterpreterError::AttributeError(format!(
            "'int' object has no attribute '{method}'"
        ))
        .into()),
    }
}
