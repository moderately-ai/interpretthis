// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `decimal` module.
//!
//! Exposes the `Decimal` class as a constructor that wraps a
//! `bigdecimal::BigDecimal`. Arithmetic is wired through the regular
//! dispatch path (Track A3 will move the actual arms there); this
//! module is the entry surface.
//!
//! Divergences from CPython documented in CONFORMANCE.md:
//!   - Context (`getcontext().prec`) is not yet exposed; arithmetic uses BigDecimal's native exact
//!     result, which matches the decimal contract for non-division operations. Division uses a
//!     bounded precision (28 digits by default — CPython's prec).
//!   - `Decimal + float` raises `TypeError`, matching CPython. `Decimal(float)` also raises
//!     `TypeError` — deliberate divergence from CPython (which accepts `Decimal(0.1)` and stores
//!     the expanded binary value); see CONFORMANCE.md#decimal-float-rejection.

use std::str::FromStr as _;

use bigdecimal::BigDecimal;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    value::Value,
};

pub fn has_function(name: &str) -> bool {
    // from_float is a Decimal classmethod only (type_classmethod), not a
    // module-level import.
    matches!(name, "Decimal")
}

/// Classmethods on `decimal.Decimal`.
#[must_use]
pub fn type_classmethod(type_name: &str, method: &str) -> Option<&'static str> {
    match (type_name, method) {
        ("Decimal", "from_float") => Some("from_float"),
        _ => None,
    }
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        "Decimal" => construct_decimal(args.first()),
        // CPython: Decimal.from_float(0.1) — explicit opt-in to the binary expansion.
        "from_float" => {
            let Some(Value::Float(f)) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "from_float() argument must be a float".into(),
                )
                .into());
            };
            // BigDecimal::try_from(f64) uses exact conversion of the binary value.
            let big = BigDecimal::try_from(*f).map_err(|_| {
                InterpreterError::ValueError(format!("cannot convert float {f} to Decimal"))
            })?;
            Ok(Value::Decimal(Box::new(big)))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'decimal' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// `decimal.Decimal(x)` — accepts an int, a string with the exact
/// digit representation, or another Decimal. `Decimal(float)` raises
/// `TypeError`; this is a **deliberate divergence** from CPython, which
/// accepts `Decimal(0.1)` and constructs the exact expanded binary
/// representation (`Decimal('0.1000000000000000055511151231257827021181583404541015625')`).
/// We reject because that expanded value almost never matches the
/// source literal the user typed. See
/// CONFORMANCE.md#decimal-float-rejection for the full divergence note.
pub(crate) fn construct_decimal(arg: Option<&Value>) -> EvalResult {
    let Some(arg) = arg else {
        return Err(InterpreterError::TypeError(
            "Decimal() requires a value (int, str, or Decimal)".into(),
        )
        .into());
    };
    let big = match arg {
        Value::Int(i) => BigDecimal::from(*i),
        Value::Bool(b) => BigDecimal::from(i64::from(*b)),
        Value::String(s) => BigDecimal::from_str(s.trim()).map_err(|e| {
            EvalError::from(InterpreterError::ValueError(format!(
                "invalid Decimal literal: {s:?} ({e})"
            )))
        })?,
        Value::Decimal(d) => (**d).clone(),
        Value::Float(_) => {
            return Err(InterpreterError::TypeError(
                "Decimal() does not accept float — use a string instead (see \
                 CONFORMANCE.md#decimal-float-rejection)"
                    .into(),
            )
            .into());
        }
        other => {
            return Err(InterpreterError::TypeError(format!(
                "Decimal() expects int / str / Decimal, got '{}'",
                other.type_name()
            ))
            .into());
        }
    };
    Ok(Value::Decimal(Box::new(big)))
}

/// `decimal` module registration.
pub struct DecimalModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for DecimalModule {
    fn name(&self) -> &'static str {
        "decimal"
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
