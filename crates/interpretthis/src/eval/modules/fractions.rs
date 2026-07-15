// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `fractions` module.
//!
//! `Fraction(numer, denom)` produces an auto-simplifying rational
//! backed by `num_rational::BigRational`. Arithmetic and comparison
//! arms live in `eval/operations.rs` (Track A3 will fold them through
//! the dispatch layer); this module is the constructor entry surface.

use std::str::FromStr as _;

use num_bigint::BigInt;
use num_rational::BigRational;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    value::Value,
};

pub fn has_function(name: &str) -> bool {
    matches!(name, "Fraction")
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        "Fraction" => construct_fraction(args),
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'fractions' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// `Fraction(int)` — denom defaults to 1.
/// `Fraction(numer, denom)` — auto-simplified.
/// `Fraction("n/d")` / `Fraction("n")` — string parse.
/// `Fraction(Fraction)` — pass through.
///
/// CPython normalises the sign to the numerator (`Fraction(3, -4)` is
/// `Fraction(-3, 4)`); `BigRational::new` does the same automatically.
pub(crate) fn construct_fraction(args: &[Value]) -> EvalResult {
    match args {
        [] => Ok(Value::Fraction(Box::new(BigRational::from_integer(BigInt::from(0))))),
        [arg] => from_single(arg),
        [numer, denom] => from_pair(numer, denom),
        _ => Err(InterpreterError::TypeError(format!(
            "Fraction() takes at most 2 arguments ({} given)",
            args.len()
        ))
        .into()),
    }
}

fn from_single(arg: &Value) -> EvalResult {
    let rational = match arg {
        Value::Int(i) => BigRational::from_integer(BigInt::from(*i)),
        Value::BigInt(b) => BigRational::from_integer((**b).clone()),
        Value::Bool(b) => BigRational::from_integer(BigInt::from(i64::from(*b))),
        Value::Fraction(f) => (**f).clone(),
        Value::String(s) => parse_fraction_str(s)?,
        Value::Float(f) => {
            // Exact rational for the binary float (CPython as_integer_ratio path).
            BigRational::from_float(*f).ok_or_else(|| {
                EvalError::from(InterpreterError::ValueError(format!(
                    "cannot convert float {f} to Fraction"
                )))
            })?
        }
        other => {
            return Err(InterpreterError::TypeError(format!(
                "Fraction() expects int / str / Fraction, got '{}'",
                other.type_name()
            ))
            .into());
        }
    };
    Ok(Value::Fraction(Box::new(rational)))
}

fn from_pair(numer: &Value, denom: &Value) -> EvalResult {
    let n = value_to_bigint(numer, "numerator")?;
    let d = value_to_bigint(denom, "denominator")?;
    if d.sign() == num_bigint::Sign::NoSign {
        return Err(crate::value::ExceptionValue::zero_division_error(
            "Fraction(_, 0): denominator is zero",
        )
        .into());
    }
    Ok(Value::Fraction(Box::new(BigRational::new(n, d))))
}

fn value_to_bigint(value: &Value, field: &str) -> Result<BigInt, EvalError> {
    // int / bool / BigInt (arbitrary precision) all accepted — the storage is a
    // BigRational, so a numerator/denominator beyond i64 is representable.
    crate::value::value_as_bigint(value).ok_or_else(|| {
        InterpreterError::TypeError(format!(
            "Fraction() {field} expects int, got '{}'",
            value.type_name()
        ))
        .into()
    })
}

fn parse_fraction_str(s: &str) -> Result<BigRational, EvalError> {
    let trimmed = s.trim();
    let invalid = || {
        EvalError::from(InterpreterError::ValueError(format!(
            "Invalid literal for Fraction: {s:?}"
        )))
    };
    // `numerator/denominator` form: the numerator is a signed integer, the
    // denominator an unsigned integer (CPython rejects `"10/-4"` and any decimal
    // in either side, e.g. `"1.5/2"`). Whitespace/underscores around each are
    // tolerated. A zero denominator raises ZeroDivisionError, not ValueError.
    if let Some((n_str, d_str)) = trimmed.split_once('/') {
        let n = parse_int_literal(n_str.trim(), true).ok_or_else(invalid)?;
        let d = parse_int_literal(d_str.trim(), false).ok_or_else(invalid)?;
        if d.sign() == num_bigint::Sign::NoSign {
            return Err(crate::value::ExceptionValue::zero_division_error(
                "Fraction(_, 0): denominator is zero",
            )
            .into());
        }
        Ok(BigRational::new(n, d))
    } else {
        parse_decimal_to_rational(trimmed).ok_or_else(invalid)
    }
}

/// Parse a plain integer literal (optional `_` digit separators). A leading
/// `+`/`-` is accepted only when `allow_sign` is set — the Fraction slash form's
/// denominator forbids a sign. Returns `None` on any non-digit content.
fn parse_int_literal(s: &str, allow_sign: bool) -> Option<BigInt> {
    let cleaned = s.replace('_', "");
    let (neg, digits) = if let Some(rest) = cleaned.strip_prefix('-') {
        allow_sign.then_some((true, rest))?
    } else if let Some(rest) = cleaned.strip_prefix('+') {
        allow_sign.then_some((false, rest))?
    } else {
        (false, cleaned.as_str())
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = BigInt::from_str(digits).ok()?;
    Some(if neg { -n } else { n })
}

/// Parse a decimal literal (`"3"`, `"0.25"`, `"-1.5"`, `"1e3"`, `"2.5e-1"`,
/// with optional `_` digit separators) into the exact rational it denotes.
/// Returns `None` on any malformed input so the caller raises the CPython
/// `Invalid literal for Fraction` error.
fn parse_decimal_to_rational(s: &str) -> Option<BigRational> {
    let s = s.trim().replace('_', "");
    if s.is_empty() {
        return None;
    }
    // Split off an optional exponent.
    let (mantissa, exp): (&str, i64) = match s.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse().ok()?),
        None => (s.as_str(), 0),
    };
    // Sign, then integer and fractional digit runs.
    let (negative, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, mantissa.strip_prefix('+').unwrap_or(mantissa)),
    };
    let (int_str, frac_str) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if int_str.is_empty() && frac_str.is_empty() {
        return None;
    }
    if !int_str.bytes().all(|b| b.is_ascii_digit()) || !frac_str.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let mut numer = BigInt::from_str(&format!("0{int_str}{frac_str}")).ok()?;
    if negative {
        numer = -numer;
    }
    // Value = numer * 10^(exp - frac_len). A non-negative net exponent scales the
    // numerator; a negative one becomes the denominator.
    let net_exp = exp - i64::try_from(frac_str.len()).ok()?;
    let pow10 = |k: u32| BigInt::from(10).pow(k);
    Some(if net_exp >= 0 {
        BigRational::from_integer(numer * pow10(u32::try_from(net_exp).ok()?))
    } else {
        BigRational::new(numer, pow10(u32::try_from(-net_exp).ok()?))
    })
}

/// `fractions` module registration.
pub struct FractionsModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for FractionsModule {
    fn name(&self) -> &'static str {
        "fractions"
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        _state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &indexmap::IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        call(func, args)
    }
}
