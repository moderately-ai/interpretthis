// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `float` method dispatch — the numeric-tower and formatting methods
//! CPython exposes on a `float` receiver (`conjugate`, `real`, `imag`,
//! `is_integer`, `as_integer_ratio`, `hex`). See the parent module's
//! `dispatch_method` for the routing hub.

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    value::Value,
};

/// Dispatch a method call on a `float` receiver. All but `__round__`
/// are argument-less.
pub(crate) fn dispatch_float_method(
    f: f64,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "conjugate" | "real" => Ok(Value::Float(f)),
        "imag" => Ok(Value::Float(0.0)),
        "is_integer" => Ok(Value::Bool(f.is_finite() && f.fract() == 0.0)),
        "as_integer_ratio" => as_integer_ratio(f),
        "hex" => Ok(Value::String(float_hex(f).into())),
        // Numeric dunders CPython exposes on floats. `__int__` /
        // `__trunc__` truncate toward zero; `__floor__` / `__ceil__`
        // return ints; the rest mirror the operators.
        "__int__" | "__trunc__" => crate::eval::functions::float_to_int_exact(f.trunc()),
        "__floor__" => crate::eval::functions::float_to_int_exact(f.floor()),
        "__ceil__" => crate::eval::functions::float_to_int_exact(f.ceil()),
        "__float__" | "__pos__" => Ok(Value::Float(f)),
        "__abs__" => Ok(Value::Float(f.abs())),
        "__neg__" => Ok(Value::Float(-f)),
        "__bool__" => Ok(Value::Bool(f != 0.0)),
        "__round__" => {
            let ndigits = match args.first() {
                None => None,
                Some(Value::Int(n)) => Some(*n),
                Some(Value::Bool(b)) => Some(i64::from(*b)),
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "'{}' object cannot be interpreted as an integer",
                        other.type_name()
                    ))
                    .into());
                }
            };
            crate::eval::functions::round_float(f, ndigits)
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'float' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// `float.as_integer_ratio()` — the exact `(numerator, denominator)` in lowest
/// terms. Infinity/NaN raise (OverflowError/ValueError), as in CPython.
fn as_integer_ratio(f: f64) -> EvalResult {
    if f.is_nan() {
        return Err(EvalError::from(InterpreterError::ValueError(
            "cannot convert NaN to integer ratio".into(),
        )));
    }
    if f.is_infinite() {
        return Err(EvalError::Exception(crate::value::ExceptionValue::new(
            "OverflowError",
            "cannot convert Infinity to integer ratio",
        )));
    }
    let ratio = num_rational::BigRational::from_float(f).ok_or_else(|| {
        EvalError::from(InterpreterError::ValueError("cannot convert float to ratio".into()))
    })?;
    Ok(Value::Tuple(vec![
        crate::value::int_from_bigint(ratio.numer().clone()),
        crate::value::int_from_bigint(ratio.denom().clone()),
    ]))
}

/// CPython's `float.hex()` — the C99 hexadecimal-float form,
/// `[-]0x1.<13 hex digits>p<±exp>` (`0x1.8000000000000p+0` for `1.5`).
fn float_hex(f: f64) -> String {
    if f.is_nan() {
        return "nan".to_string();
    }
    if f.is_infinite() {
        return if f < 0.0 { "-inf".to_string() } else { "inf".to_string() };
    }
    let sign = if f.is_sign_negative() { "-" } else { "" };
    if f == 0.0 {
        return format!("{sign}0x0.0p+0");
    }
    let bits = f.to_bits();
    let exp_bits = ((bits >> 52) & 0x7ff) as i64;
    let mantissa = bits & 0x000f_ffff_ffff_ffff;
    // Normal numbers carry an implicit leading 1; subnormals a leading 0 with a
    // fixed minimum exponent.
    let (lead, exp) = if exp_bits == 0 { (0u64, -1022) } else { (1u64, exp_bits - 1023) };
    format!("{sign}0x{lead}.{mantissa:013x}p{exp:+}")
}
