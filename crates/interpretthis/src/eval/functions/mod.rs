// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{
    error::{EvalError, InterpreterError},
    value::Value,
};

pub(crate) mod methods;
pub(crate) mod params;

mod builtins;
mod call;
mod definitions;
pub(crate) mod dispatch;
mod generators;
pub(crate) mod helpers;
mod method_dispatch;

// Re-exports from builtins
pub use builtins::is_exception_type_name;
// Re-exports from call
pub use call::eval_call;
pub(crate) use definitions::{
    VariableCheckpoint, collect_assigned_names, collect_free_names, contains_yield_stmts,
    extract_docstring, extract_function_source,
};
// Re-exports from definitions
pub use definitions::{build_function_params, eval_function_def, eval_lambda_def};
// Re-exports from dispatch
pub(crate) use dispatch::{call_lambda, call_user_function, call_value_as_function};
// Re-exports from method_dispatch
pub(crate) use generators::{
    create_generator, create_synthetic_generator, dispatch_generator_method, finalize_generators,
    generator_suspendable, is_generator_method,
};
pub(crate) use method_dispatch::{
    CallArgs, MethodOutcome, arg1, bind_method_params, reject_kwargs, require_param,
};
pub(crate) use params::{bind_params_named, evaluate_param_defaults, execute_body};

/// Convert a Python-visible `i64` index into a `usize` slot after caller-side
/// sign-and-bounds validation. Fails with a clean `RuntimeError` on invariant
/// violation — makes the invariant explicit at the cast site rather than
/// silently truncating or sign-wrapping via `as`.
pub(crate) fn to_index(i: i64) -> Result<usize, EvalError> {
    usize::try_from(i)
        .map_err(|_| InterpreterError::Runtime("index overflow or negative".into()).into())
}

/// Convert a container length into `i64` for Python-signed index arithmetic.
/// Fails cleanly if the length exceeds `i64::MAX` (effectively never for
/// real data, but the `try_from` keeps the invariant explicit).
pub(crate) fn to_len_i64(len: usize) -> Result<i64, EvalError> {
    i64::try_from(len)
        .map_err(|_| InterpreterError::Runtime("collection length overflows i64".into()).into())
}

/// Resolve the optional `start`/`end` arguments of a sequence `list`/`tuple`
/// `.index(value, start, stop)` into a half-open `[start, end)` slot range over
/// `len` elements. Unlike the `str` search family, these bounds are
/// integer-only (CPython raises `TypeError` on `None`), so they route through
/// `value_to_i64`; negative indices count from the end and clamp to `[0, len]`.
pub(crate) fn sequence_index_range(
    method: &str,
    args: &[Value],
    len: usize,
) -> Result<(usize, usize), EvalError> {
    if args.is_empty() || args.len() > 3 {
        return Err(
            InterpreterError::TypeError(format!("{method}() takes 1 to 3 arguments")).into()
        );
    }
    let len_i = to_len_i64(len)?;
    let clamp = |v: i64| -> i64 {
        let v = if v < 0 { v + len_i } else { v };
        v.clamp(0, len_i)
    };
    let start = match args.get(1) {
        None => 0,
        Some(v) => clamp(value_to_i64(v)?),
    };
    let end = match args.get(2) {
        None => len_i,
        Some(v) => clamp(value_to_i64(v)?),
    };
    Ok((to_index(start)?, to_index(end.max(start))?))
}

/// Optional integer index argument: missing or `None` → `None` (use the
/// default); otherwise coerced via `value_to_i64` (non-integers raise
/// `TypeError`). Shared by the `str`/`bytes` search-method families, whose
/// `start`/`end` bounds accept `None` (unlike `list`/`tuple` `.index`).
pub(crate) fn opt_index_arg(arg: Option<&Value>) -> Result<Option<i64>, EvalError> {
    match arg {
        None | Some(Value::None) => Ok(None),
        Some(v) => Ok(Some(value_to_i64(v)?)),
    }
}

/// Python's `int(float)`: truncate toward zero to the *exact* integer.
///
/// - `NaN` raises `ValueError` (CPython: "cannot convert float NaN to integer").
/// - `±inf` raises `OverflowError` ("cannot convert float infinity to integer").
/// - A finite value converts exactly, promoting past `i64` to `BigInt` rather
///   than saturating — `int(1e30)` is `1000000000000000019884624838656`, the
///   exact integer the float represents, not `i64::MAX`.
pub(crate) fn float_to_int_exact(f: f64) -> Result<Value, EvalError> {
    use num_traits::FromPrimitive as _;
    if f.is_nan() {
        return Err(EvalError::Exception(crate::value::ExceptionValue::new(
            "ValueError",
            "cannot convert float NaN to integer",
        )));
    }
    if f.is_infinite() {
        return Err(EvalError::Exception(crate::value::ExceptionValue::new(
            "OverflowError",
            "cannot convert float infinity to integer",
        )));
    }
    let truncated = f.trunc();
    let big = num_bigint::BigInt::from_f64(truncated).ok_or_else(|| {
        EvalError::from(InterpreterError::ValueError("cannot convert float to integer".into()))
    })?;
    Ok(crate::value::int_from_bigint(big))
}

/// `round(int, ndigits)` — CPython's banker's rounding to the nearest
/// multiple of `10**(-ndigits)`. `None` / non-negative `ndigits` is a
/// no-op; negative rounds off low-order decimal digits. Shared by the
/// `round` builtin and `int.__round__`.
pub(crate) fn round_int(i: i64, ndigits: Option<i64>) -> Value {
    let Some(n) = ndigits else { return Value::Int(i) };
    if n >= 0 {
        return Value::Int(i);
    }
    let abs_exp = u32::try_from(-n).unwrap_or(u32::MAX);
    // |n| beyond ~19 wipes any i64 out to zero; CPython returns 0 too.
    if abs_exp > 18 {
        return Value::Int(0);
    }
    let factor = 10_i64.pow(abs_exp);
    // Banker's round: truncated divide, then on an exact half pick the
    // even quotient. Rust's `/` truncates toward zero, so negatives step
    // away from zero on a round-up.
    let q = i / factor;
    let r = i - q * factor;
    let twice_r = r.abs() * 2;
    let rounded = match twice_r.cmp(&factor) {
        std::cmp::Ordering::Equal => {
            if q % 2 == 0 {
                q
            } else if i.is_negative() {
                q - 1
            } else {
                q + 1
            }
        }
        std::cmp::Ordering::Greater => {
            if i.is_negative() {
                q - 1
            } else {
                q + 1
            }
        }
        std::cmp::Ordering::Less => q,
    };
    Value::Int(rounded * factor)
}

/// Banker's-rounding of a `BigInt` to the nearest `10**(-ndigits)`.
/// `None` / non-negative `ndigits` is a no-op. Shared by the `round`
/// builtin and `int.__round__` on out-of-i64 receivers.
pub(crate) fn round_bigint(b: &num_bigint::BigInt, ndigits: Option<i64>) -> num_bigint::BigInt {
    let Some(n) = ndigits else { return b.clone() };
    if n >= 0 {
        return b.clone();
    }
    let abs_exp = u32::try_from(-n).unwrap_or(u32::MAX);
    let factor = num_bigint::BigInt::from(10).pow(abs_exp);
    // Truncating div/rem (toward zero), then resolve an exact half to
    // the even quotient — mirrors the i64 path in `round_int`.
    let q = b / &factor;
    let r = b - &q * &factor;
    let twice_r = r.magnitude() * 2u32;
    let rounded = match twice_r.cmp(factor.magnitude()) {
        std::cmp::Ordering::Equal => {
            if num_traits::Zero::is_zero(&(&q % 2)) {
                q
            } else if b.sign() == num_bigint::Sign::Minus {
                q - 1
            } else {
                q + 1
            }
        }
        std::cmp::Ordering::Greater => {
            if b.sign() == num_bigint::Sign::Minus {
                q - 1
            } else {
                q + 1
            }
        }
        std::cmp::Ordering::Less => q,
    };
    rounded * factor
}

/// `round(float, ndigits)` — CPython's IEEE-754 round-half-to-even.
/// `ndigits is None` returns an int; otherwise a float rounded to that
/// many decimal places (negative places round to the nearest 10^|n|).
/// Shared by the `round` builtin and `float.__round__`.
pub(crate) fn round_float(f: f64, ndigits: Option<i64>) -> Result<Value, EvalError> {
    let Some(n) = ndigits else {
        // `round(x)` with no ndigits yields an int.
        return float_to_int_exact(f.round_ties_even());
    };
    // CPython's `round(x, n>=0)` uses correctly-rounded decimal
    // formatting (dtoa), not multiply-round-divide, so 2.675 → 2.67.
    // Rust's `{:.n$}` formatter shares that algorithm.
    if n >= 0 {
        let places = usize::try_from(n).unwrap_or(usize::MAX);
        let s = format!("{f:.places$}");
        let parsed = s.parse::<f64>().unwrap_or(f);
        return Ok(Value::Float(parsed));
    }
    // Negative ndigits: round to the nearest 10^|n|. Divide (rather than
    // multiply by 10^n) so the scaled value stays finite.
    let abs_exp = i32::try_from(-n).unwrap_or(i32::MAX);
    let pow10 = 10f64.powi(abs_exp);
    if !pow10.is_finite() {
        return Ok(Value::Float(0.0_f64.copysign(f)));
    }
    Ok(Value::Float((f / pow10).round_ties_even() * pow10))
}

/// Round a reduced `BigRational` to the nearest integer with round-half-to-even,
/// mirroring `Fraction.__round__(None)`.
fn round_ratio_bankers(r: &num_rational::BigRational) -> num_bigint::BigInt {
    use num_integer::Integer as _;
    // `div_mod_floor` gives a remainder in `[0, denom)` since the denominator is
    // positive, so the tie test `2*rem == denom` is sign-agnostic.
    let (floor, rem) = r.numer().div_mod_floor(r.denom());
    let twice = &rem * 2u32;
    match twice.cmp(r.denom()) {
        std::cmp::Ordering::Less => floor,
        std::cmp::Ordering::Greater => floor + 1,
        std::cmp::Ordering::Equal => {
            if floor.is_even() {
                floor
            } else {
                floor + 1
            }
        }
    }
}

/// `round(Fraction, ndigits)` — `None` yields an int (round-half-to-even), any
/// `ndigits` yields a `Fraction` rounded to that many decimal places.
pub(crate) fn round_fraction(fr: &num_rational::BigRational, ndigits: Option<i64>) -> Value {
    use num_rational::BigRational;
    let Some(n) = ndigits else {
        return crate::value::int_from_bigint(round_ratio_bankers(fr));
    };
    let ten = num_bigint::BigInt::from(10);
    if n >= 0 {
        let pow = ten.pow(u32::try_from(n).unwrap_or(u32::MAX));
        let scaled = fr * BigRational::from_integer(pow.clone());
        let rounded = round_ratio_bankers(&scaled);
        Value::Fraction(Box::new(BigRational::new(rounded, pow)))
    } else {
        let pow = ten.pow(u32::try_from(-n).unwrap_or(u32::MAX));
        let scaled = fr / BigRational::from_integer(pow.clone());
        let rounded = round_ratio_bankers(&scaled);
        Value::Fraction(Box::new(BigRational::from_integer(rounded * pow)))
    }
}

/// `round(Decimal, ndigits)` — `None` yields an int (round-half-to-even), any
/// `ndigits` yields a `Decimal` rounded to that scale.
pub(crate) fn round_decimal(d: &bigdecimal::BigDecimal, ndigits: Option<i64>) -> Value {
    use bigdecimal::RoundingMode::HalfEven;
    match ndigits {
        None => {
            let rounded = d.with_scale_round(0, HalfEven);
            crate::value::int_from_bigint(rounded.as_bigint_and_exponent().0)
        }
        Some(n) => Value::Decimal(Box::new(d.with_scale_round(n, HalfEven)), false),
    }
}

// ---------------------------------------------------------------------------
// Proxy resolution
// ---------------------------------------------------------------------------

/// Resolve a Value if it's a `LazyProxy`, otherwise return as-is.
pub async fn resolve_proxy(value: &Value) -> Result<Value, EvalError> {
    if let Value::LazyProxy(proxy) = value {
        proxy.resolve().await.map_err(|e| {
            EvalError::Interpreter(InterpreterError::Tool {
                tool_name: proxy.tool_name.clone(),
                message: e.message,
            })
        })
    } else {
        Ok(value.clone())
    }
}

pub(crate) fn check_arg_count(
    name: &str,
    args: &[Value],
    min: usize,
    max: usize,
) -> Result<(), EvalError> {
    if args.len() < min || args.len() > max {
        if min == max {
            return Err(InterpreterError::TypeError(format!(
                "{name}() takes exactly {min} argument(s) ({} given)",
                args.len()
            ))
            .into());
        }
        return Err(InterpreterError::TypeError(format!(
            "{name}() takes {min} to {max} arguments ({} given)",
            args.len()
        ))
        .into());
    }
    Ok(())
}

/// Read an integer-valued argument for a builtin that expects an `int`.
///
/// A `float` is deliberately NOT accepted: an integer parameter (a `range`
/// bound, a `chr` code point, a list index, a field width) rejects a float in
/// CPython with `TypeError: 'float' object cannot be interpreted as an integer`.
/// Only `int(float)` truncates — that is the `int()` builtin's own path, which
/// does not go through here. Accepting floats here silently turned `range(2.9)`
/// into `range(2)`.
pub(crate) fn value_to_i64(val: &Value) -> Result<i64, EvalError> {
    match val {
        Value::Int(i) => Ok(*i),
        Value::Bool(b) => Ok(i64::from(*b)),
        // IntEnum / IntFlag members interpret as their underlying int; a plain
        // Enum / Flag is not an integer and falls through to the type error.
        Value::EnumMember {
            value,
            kind: crate::value::EnumKind::Int | crate::value::EnumKind::IntFlag,
            ..
        } => value_to_i64(value),
        // Every non-integer (float, str, list, …) reports the same CPython
        // message; only `int(float)` truncates, and that path does not go here.
        _ => Err(InterpreterError::TypeError(format!(
            "'{}' object cannot be interpreted as an integer",
            val.type_name()
        ))
        .into()),
    }
}
