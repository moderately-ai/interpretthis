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

use bigdecimal::BigDecimal;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    value::Value,
};

/// Dispatch a method call on a `Decimal` receiver.
pub(crate) fn dispatch_decimal_method(d: &BigDecimal, method: &str, args: &[Value]) -> EvalResult {
    use num_traits::{Signed as _, Zero as _};
    match method {
        // `quantize(exp)` rounds to the exponent (decimal scale) of `exp`,
        // using banker's rounding (CPython's default ROUND_HALF_EVEN).
        "quantize" => {
            let Some(Value::Decimal(exp)) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "quantize() requires a Decimal argument".into(),
                )
                .into());
            };
            let scale = exp.fractional_digit_count();
            Ok(Value::Decimal(Box::new(
                d.with_scale_round(scale, bigdecimal::RoundingMode::HalfEven),
            )))
        }
        "copy_abs" => Ok(Value::Decimal(Box::new(d.abs()))),
        "copy_negate" => Ok(Value::Decimal(Box::new(-d.clone()))),
        "copy_sign" => {
            let Some(Value::Decimal(other)) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "copy_sign() requires a Decimal argument".into(),
                )
                .into());
            };
            let magnitude = d.abs();
            Ok(Value::Decimal(Box::new(if other.is_negative() { -magnitude } else { magnitude })))
        }
        "is_zero" => Ok(Value::Bool(d.is_zero())),
        "is_signed" => Ok(Value::Bool(d.is_negative())),
        "is_nan" | "is_infinite" | "is_qnan" | "is_snan" => Ok(Value::Bool(false)),
        "normalize" => Ok(Value::Decimal(Box::new(d.normalized()))),
        "sqrt" => d.sqrt().map(|r| Value::Decimal(Box::new(r))).ok_or_else(|| {
            EvalError::Exception(crate::value::ExceptionValue::new(
                "InvalidOperation",
                "sqrt of negative Decimal",
            ))
        }),
        "to_integral_value" | "to_integral" => {
            Ok(Value::Decimal(Box::new(d.with_scale_round(0, bigdecimal::RoundingMode::HalfEven))))
        }
        "as_integer_ratio" => {
            let (mantissa, scale) = d.as_bigint_and_exponent();
            let (num, den) = if scale >= 0 {
                (mantissa, num_bigint::BigInt::from(10).pow(u32::try_from(scale).unwrap_or(0)))
            } else {
                (
                    mantissa * num_bigint::BigInt::from(10).pow(u32::try_from(-scale).unwrap_or(0)),
                    num_bigint::BigInt::from(1),
                )
            };
            let ratio = num_rational::BigRational::new(num, den);
            Ok(Value::Fraction(Box::new(ratio)))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'Decimal' object has no attribute '{method}'"
        ))
        .into()),
    }
}

pub const CONTEXT_CLASS: &str = "decimal.Context";
/// Marker class for `with localcontext() as ctx:`.
pub const LOCAL_CONTEXT_CLASS: &str = "decimal.LocalContext";

pub fn has_function(name: &str) -> bool {
    // from_float is a Decimal classmethod only (type_classmethod), not a
    // module-level import.
    matches!(name, "Decimal" | "getcontext" | "setcontext" | "localcontext")
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
        "getcontext" => Ok(make_context_instance(state)),
        "localcontext" => {
            ensure_context_class(state);
            // Optional Context arg: if provided, its prec is applied on enter.
            let mut fields = std::collections::BTreeMap::new();
            fields.insert("saved_prec".into(), Value::Int(state.decimal_prec));
            if let Some(Value::Instance(inst)) = args.first() {
                if inst.class_name == CONTEXT_CLASS {
                    if let Some(Value::Int(n)) = inst.fields.lock().get("prec") {
                        fields.insert("enter_prec".into(), Value::Int(*n));
                    }
                }
            }
            // Also expose a nested Context for `as ctx` that mutates state.
            fields.insert("ctx".into(), make_context_instance(state));
            Ok(Value::Instance(crate::value::InstanceValue {
                class_name: LOCAL_CONTEXT_CLASS.into(),
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

fn make_context_instance(state: &mut crate::state::InterpreterState) -> Value {
    ensure_context_class(state);
    let mut fields = std::collections::BTreeMap::new();
    fields.insert("prec".into(), Value::Int(state.decimal_prec));
    // rounding accepted for read/write; only ROUND_HALF_EVEN is effective today.
    fields.insert("rounding".into(), Value::String("ROUND_HALF_EVEN".into()));
    Value::Instance(crate::value::InstanceValue {
        class_name: CONTEXT_CLASS.into(),
        fields: crate::value::shared_fields(fields),
    })
}

/// `with localcontext() as ctx` enter/exit special-case (saves/restores prec).
pub(crate) fn try_localcontext_method(
    state: &mut crate::state::InterpreterState,
    receiver: &Value,
    method: &str,
    _args: &[Value],
) -> Option<EvalResult> {
    let Value::Instance(inst) = receiver else {
        return None;
    };
    if inst.class_name != LOCAL_CONTEXT_CLASS {
        return None;
    }
    match method {
        "__enter__" => {
            let fields = inst.fields.lock();
            if let Some(Value::Int(n)) = fields.get("enter_prec") {
                if *n >= 1 {
                    state.decimal_prec = *n;
                }
            }
            // Return the nested Context instance for `as ctx`.
            let ctx = fields.get("ctx").cloned().unwrap_or(Value::None);
            // Refresh ctx.prec to current state.
            drop(fields);
            if let Value::Instance(ctx_inst) = &ctx {
                ctx_inst.fields.lock().insert("prec".into(), Value::Int(state.decimal_prec));
            }
            Some(Ok(ctx))
        }
        "__exit__" => {
            let fields = inst.fields.lock();
            if let Some(Value::Int(n)) = fields.get("saved_prec") {
                state.decimal_prec = *n;
            }
            Some(Ok(Value::Bool(false)))
        }
        _ => Some(Err(InterpreterError::AttributeError(format!(
            "'LocalContext' object has no attribute '{method}'"
        ))
        .into())),
    }
}

fn ensure_context_class(state: &mut crate::state::InterpreterState) {
    use crate::value::ClassValue;
    if !state.classes.contains_key(CONTEXT_CLASS) {
        state.classes.insert(CONTEXT_CLASS.to_string(), ClassValue::new(CONTEXT_CLASS));
    }
    if !state.classes.contains_key(LOCAL_CONTEXT_CLASS) {
        state.classes.insert(LOCAL_CONTEXT_CLASS.to_string(), ClassValue::new(LOCAL_CONTEXT_CLASS));
    }
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
