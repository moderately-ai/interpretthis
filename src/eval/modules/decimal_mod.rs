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
//!   - `getcontext()` / `setcontext()` expose a Context with mutable `prec` (default 28).
//!     Division rounds to active prec; other arithmetic stays exact.
//!   - Full Context traps/rounding modes are not modelled.
//!   - `Decimal + float` raises `TypeError`, matching CPython. `Decimal(float)` also raises
//!     `TypeError` — deliberate divergence from CPython (which accepts `Decimal(0.1)` and stores
//!     the expanded binary value); see CONFORMANCE.md#decimal-float-rejection.

use std::str::FromStr as _;

use std::sync::atomic::{AtomicI64, Ordering};

use bigdecimal::BigDecimal;

/// Active precision for Decimal division (mirrored from InterpreterState).
static DECIMAL_PREC: AtomicI64 = AtomicI64::new(28);

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    value::Value,
};

pub const CONTEXT_CLASS: &str = "decimal.Context";

pub fn has_function(name: &str) -> bool {
    // from_float is a Decimal classmethod only (type_classmethod), not a
    // module-level import.
    matches!(name, "Decimal" | "getcontext" | "setcontext")
}

/// Classmethods on `decimal.Decimal`.
#[must_use]
pub fn type_classmethod(type_name: &str, method: &str) -> Option<&'static str> {
    match (type_name, method) {
        ("Decimal", "from_float") => Some("from_float"),
        _ => None,
    }
}

pub fn call(func: &str, args: &[Value], state: &mut crate::state::InterpreterState) -> EvalResult {
    match func {
        "Decimal" => construct_decimal(args.first()),
        "getcontext" => {
            ensure_context_class(state);
            let mut fields = std::collections::BTreeMap::new();
            fields.insert("prec".into(), Value::Int(state.decimal_prec));
            Ok(Value::Instance(crate::value::InstanceValue {
                class_name: CONTEXT_CLASS.into(),
                fields: crate::value::shared_fields(fields),
            }))
        }
        "setcontext" => {
            let Some(Value::Instance(inst)) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "setcontext() argument must be a Context".into(),
                )
                .into());
            };
            if inst.class_name != CONTEXT_CLASS {
                return Err(InterpreterError::TypeError(
                    "setcontext() argument must be a Context".into(),
                )
                .into());
            }
            if let Some(Value::Int(n)) = inst.fields.lock().get("prec") {
                if *n < 1 {
                    return Err(InterpreterError::ValueError(
                        "valid range for prec is [1, MAX_PREC]".into(),
                    )
                    .into());
                }
                state.decimal_prec = *n;
                DECIMAL_PREC.store(*n, Ordering::Relaxed);
            }
            Ok(Value::None)
        }
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

fn ensure_context_class(state: &mut crate::state::InterpreterState) {
    use crate::value::ClassValue;
    if state.classes.contains_key(CONTEXT_CLASS) {
        return;
    }
    state.classes.insert(
        CONTEXT_CLASS.to_string(),
        ClassValue {
            name: CONTEXT_CLASS.to_string(),
            methods: Default::default(),
            class_attrs: Default::default(),
            bases: Vec::new(),
            mro: vec![CONTEXT_CLASS.to_string()],
            properties: Default::default(),
            static_methods: Default::default(),
            class_methods: Default::default(),
            enum_kind: None,
            annotations: Vec::new(),
            dataclass_fields: None,
            frozen: false,
            order: false,
            slots: false,
        },
    );
}

/// Active decimal precision for division (default 28).
#[must_use]
pub(crate) fn active_prec() -> i64 {
    DECIMAL_PREC.load(Ordering::Relaxed)
}

pub(crate) fn store_prec(n: i64) {
    DECIMAL_PREC.store(n, Ordering::Relaxed);
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
        state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &indexmap::IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        call(func, args, state)
    }
}
