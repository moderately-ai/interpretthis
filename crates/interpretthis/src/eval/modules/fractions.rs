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
        return Err(InterpreterError::Runtime("Fraction(_, 0) — denominator is zero".into()).into());
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
    if let Some((n_str, d_str)) = trimmed.split_once('/') {
        let n = BigInt::from_str(n_str.trim()).map_err(|e| {
            EvalError::from(InterpreterError::ValueError(format!("Fraction numerator parse: {e}")))
        })?;
        let d = BigInt::from_str(d_str.trim()).map_err(|e| {
            EvalError::from(InterpreterError::ValueError(format!(
                "Fraction denominator parse: {e}"
            )))
        })?;
        if d.sign() == num_bigint::Sign::NoSign {
            return Err(InterpreterError::Runtime("Fraction denominator is zero".into()).into());
        }
        Ok(BigRational::new(n, d))
    } else {
        let n = BigInt::from_str(trimmed).map_err(|e| {
            EvalError::from(InterpreterError::ValueError(format!("Fraction literal parse: {e}")))
        })?;
        Ok(BigRational::from_integer(n))
    }
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
