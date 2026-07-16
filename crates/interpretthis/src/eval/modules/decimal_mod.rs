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
pub(crate) fn dispatch_decimal_method(
    d: &BigDecimal,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    use num_traits::{Signed as _, Zero as _};
    match method {
        // `quantize(exp, rounding=…)` rounds to the exponent (decimal scale)
        // of `exp`. The rounding mode defaults to the context's
        // ROUND_HALF_EVEN (banker's) and may be overridden positionally or by
        // keyword with a `decimal.ROUND_*` string constant.
        "quantize" => {
            let Some(Value::Decimal(exp, _)) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "quantize() requires a Decimal argument".into(),
                )
                .into());
            };
            let rounding = match args.get(1).or_else(|| kwargs.get("rounding")) {
                Some(Value::String(s)) => rounding_mode(s)?,
                Some(Value::None) | None => bigdecimal::RoundingMode::HalfEven,
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "quantize() rounding must be a decimal.ROUND_* constant, not '{}'",
                        other.type_name()
                    ))
                    .into());
                }
            };
            let scale = exp.fractional_digit_count();
            Ok(Value::Decimal(Box::new(d.with_scale_round(scale, rounding)), false))
        }
        "copy_abs" => Ok(Value::Decimal(Box::new(d.abs()), false)),
        "copy_negate" => Ok(Value::Decimal(Box::new(-d.clone()), false)),
        "copy_sign" => {
            let Some(Value::Decimal(other, _)) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "copy_sign() requires a Decimal argument".into(),
                )
                .into());
            };
            let magnitude = d.abs();
            Ok(Value::Decimal(
                Box::new(if other.is_negative() { -magnitude } else { magnitude }),
                false,
            ))
        }
        "is_zero" => Ok(Value::Bool(d.is_zero())),
        "is_signed" => Ok(Value::Bool(d.is_negative())),
        "is_nan" | "is_infinite" | "is_qnan" | "is_snan" => Ok(Value::Bool(false)),
        "normalize" => Ok(Value::Decimal(Box::new(d.normalized()), false)),
        // Rounded to the default context precision (28 significant digits),
        // matching CPython — bigdecimal's raw sqrt keeps ~100 digits.
        "sqrt" => {
            let r = d.sqrt().ok_or_else(|| {
                EvalError::Exception(crate::value::ExceptionValue::new(
                    "InvalidOperation",
                    "sqrt of negative Decimal",
                ))
            })?;
            // A perfect square takes the ideal exponent (operand exponent // 2)
            // rather than padding to the context's 28 significant digits, so
            // `Decimal('9').sqrt()` is `3` and `Decimal('9.00').sqrt()` is `3.0`
            // (scale = ceil(operand_scale / 2)). Inexact roots keep 28 digits.
            let exact = ((&r * &r) - d).is_zero();
            let result = if exact {
                r.with_scale((d.fractional_digit_count() + 1) / 2)
            } else {
                r.with_prec(28)
            };
            Ok(Value::Decimal(Box::new(result), false))
        }
        // Transcendentals at the default context precision (28 significant
        // digits). `exp` is bigdecimal's; `ln`/`log10` use Newton's method on
        // `exp`. Accurate to 28 digits but — like `math.erf` — not guaranteed
        // bit-identical to CPython's correctly-rounded result in the final ULP.
        // exp(0) is exactly 1 (CPython returns the bare `1`, not `1.000…`).
        "exp" if d.is_zero() => Ok(Value::Decimal(Box::new(BigDecimal::from(1)), false)),
        "exp" => Ok(Value::Decimal(Box::new(d.exp().with_prec(28)), false)),
        "ln" => {
            // ln(1) is exactly 0 (CPython returns the bare `0`, not a padded
            // 28-digit form). Every other value gives an irrational result.
            if power_of_ten_exponent(d) == Some(0) {
                Ok(Value::Decimal(Box::new(BigDecimal::from(0)), false))
            } else {
                decimal_ln_prec(d, 45)
                    .map(|r| Value::Decimal(Box::new(r.with_prec(28)), false))
                    .ok_or_else(|| non_positive_log_error("ln"))
            }
        }
        "log10" => {
            // log10 of an exact power of ten is the exact integer exponent
            // (CPython: `Decimal(1000).log10()` is `3`, not `3.000…`).
            if let Some(k) = power_of_ten_exponent(d) {
                Ok(Value::Decimal(Box::new(BigDecimal::from(k)), false))
            } else {
                let ten = BigDecimal::from(10);
                match (decimal_ln_prec(d, 48), decimal_ln_prec(&ten, 48)) {
                    (Some(ln_d), Some(ln_ten)) => {
                        Ok(Value::Decimal(Box::new((ln_d / ln_ten).with_prec(28)), false))
                    }
                    _ => Err(non_positive_log_error("log10")),
                }
            }
        }
        // `compare(other)` yields Decimal(-1 / 0 / 1) (not a bare int).
        "compare" => {
            let Some(Value::Decimal(other, _)) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "compare() requires a Decimal argument".into(),
                )
                .into());
            };
            let sign = match d.cmp(other) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            Ok(Value::Decimal(Box::new(BigDecimal::from(sign)), false))
        }
        // `scaleb(n)` shifts the decimal exponent by `n` (multiply by 10**n)
        // while preserving the coefficient, so the E-notation is retained.
        "scaleb" => {
            let n = match args.first() {
                Some(Value::Int(i)) => *i,
                Some(Value::Decimal(dec, _)) => {
                    use num_traits::ToPrimitive as _;
                    dec.to_i64().unwrap_or(0)
                }
                _ => {
                    return Err(InterpreterError::TypeError(
                        "scaleb() requires an integer or Decimal argument".into(),
                    )
                    .into());
                }
            };
            let (mantissa, scale) = d.as_bigint_and_exponent();
            Ok(Value::Decimal(Box::new(BigDecimal::new(mantissa, scale - n)), false))
        }
        "to_integral_value" | "to_integral" => Ok(Value::Decimal(
            Box::new(d.with_scale_round(0, bigdecimal::RoundingMode::HalfEven)),
            false,
        )),
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
            // CPython returns a plain `(numerator, denominator)` int tuple in
            // lowest terms, not a Fraction. `BigRational::new` reduces for us.
            let ratio = num_rational::BigRational::new(num, den);
            Ok(Value::Tuple(vec![
                crate::value::int_from_bigint(ratio.numer().clone()),
                crate::value::int_from_bigint(ratio.denom().clone()),
            ]))
        }
        // `as_tuple()` -> DecimalTuple(sign, digits, exponent): value ==
        // (-1)^sign * digits * 10^exponent, with trailing zeros preserved
        // (`Decimal('1.00')` -> digits (1, 0, 0), exponent -2). Signless zero
        // reports sign 0 — the `-0.0` case cannot be recovered from the bare
        // BigDecimal here, matching `is_signed`/`copy_sign`.
        "as_tuple" => {
            let (mantissa, scale) = d.as_bigint_and_exponent();
            let sign = i64::from(mantissa.is_negative());
            let digits: Vec<Value> = mantissa
                .abs()
                .to_string()
                .bytes()
                .map(|b| Value::Int(i64::from(b - b'0')))
                .collect();
            let mut fields = std::collections::BTreeMap::new();
            fields.insert("sign".to_string(), Value::Int(sign));
            fields.insert("digits".to_string(), Value::Tuple(digits));
            fields.insert("exponent".to_string(), Value::Int(-scale));
            Ok(Value::Instance(crate::value::InstanceValue {
                class_name: "DecimalTuple".to_string(),
                fields: crate::value::shared_fields(fields),
            }))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'Decimal' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// `InvalidOperation` for `ln`/`log10` of a value that is not strictly
/// positive, matching CPython's error class.
fn non_positive_log_error(op: &str) -> EvalError {
    EvalError::Exception(crate::value::ExceptionValue::new(
        "InvalidOperation",
        format!("{op} of a non-positive value"),
    ))
}

/// If `d` is an exact power of ten (`10**k`), return `k`; else `None`. Used to
/// short-circuit `log10` (and `ln(1)`) to an exact integer result, matching
/// CPython's special-casing.
fn power_of_ten_exponent(d: &BigDecimal) -> Option<i64> {
    let (mantissa, scale) = d.normalized().into_bigint_and_exponent();
    (mantissa == num_bigint::BigInt::from(1)).then_some(-scale)
}

/// Natural logarithm of a positive `BigDecimal`, computed to `guard`
/// significant digits by Newton's method on `exp`
/// (`y ← y + d/exp(y) − 1`, which solves `exp(y) = d`). Quadratically
/// convergent from an `f64` seed. Returns `None` for a non-positive input.
/// The result carries `guard` digits so callers can round to the context
/// precision (or divide, for `log10`) without compounding rounding error.
fn decimal_ln_prec(d: &BigDecimal, guard: u64) -> Option<BigDecimal> {
    use num_traits::{FromPrimitive as _, Signed as _, ToPrimitive as _};
    if !d.is_positive() {
        return None;
    }
    let one = BigDecimal::from(1);
    let mut y = BigDecimal::from_f64(d.to_f64()?.ln())?;
    // Newton converges quadratically; from ~15 f64 digits, a handful of
    // iterations reaches `guard`. Cap the loop as a safety bound.
    for _ in 0..40 {
        let ey = y.exp().with_prec(guard);
        let ratio = (d / &ey).with_prec(guard);
        let correction = (ratio - &one).with_prec(guard);
        let y_next = (&y + &correction).with_prec(guard);
        if y_next == y {
            break;
        }
        y = y_next;
    }
    Some(y)
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
            Ok(Value::Decimal(Box::new(big), false))
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
    // CPython keeps the sign of a zero Decimal (`Decimal('-0.0')` prints
    // `-0.0`), but `bigdecimal` normalises it away. Track it separately.
    let (big, neg_zero) = match arg {
        Value::Int(i) => (BigDecimal::from(*i), false),
        Value::Bool(b) => (BigDecimal::from(i64::from(*b)), false),
        Value::String(s) => {
            use num_traits::Zero as _;
            let trimmed = s.trim();
            let bd = BigDecimal::from_str(trimmed).map_err(|e| {
                EvalError::from(InterpreterError::ValueError(format!(
                    "invalid Decimal literal: {s:?} ({e})"
                )))
            })?;
            let neg_zero = bd.is_zero() && trimmed.starts_with('-');
            (bd, neg_zero)
        }
        Value::Decimal(d, nz) => ((**d).clone(), *nz),
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
    Ok(Value::Decimal(Box::new(big), neg_zero))
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
    fn constant(&self, name: &str) -> Option<Value> {
        rounding_constant(name)
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

/// The `decimal` rounding-mode constants. CPython models each as a string
/// equal to its own name (`decimal.ROUND_HALF_UP == 'ROUND_HALF_UP'`), so
/// `quantize(exp, rounding=ROUND_HALF_UP)` receives the mode as a string.
/// Map a `decimal.ROUND_*` string constant to a `bigdecimal` rounding mode.
/// `ROUND_05UP` (round to nearest 0/5) has no `bigdecimal` equivalent and is
/// approximated by round-away-from-zero — a documented rare-mode divergence.
fn rounding_mode(name: &str) -> Result<bigdecimal::RoundingMode, EvalError> {
    use bigdecimal::RoundingMode;
    Ok(match name {
        "ROUND_CEILING" => RoundingMode::Ceiling,
        "ROUND_DOWN" => RoundingMode::Down,
        "ROUND_FLOOR" => RoundingMode::Floor,
        "ROUND_HALF_DOWN" => RoundingMode::HalfDown,
        "ROUND_HALF_EVEN" => RoundingMode::HalfEven,
        "ROUND_HALF_UP" => RoundingMode::HalfUp,
        "ROUND_UP" | "ROUND_05UP" => RoundingMode::Up,
        _ => {
            return Err(
                InterpreterError::ValueError(format!("invalid rounding mode: {name}")).into()
            );
        }
    })
}

pub(crate) fn rounding_constant(name: &str) -> Option<Value> {
    matches!(
        name,
        "ROUND_CEILING"
            | "ROUND_DOWN"
            | "ROUND_FLOOR"
            | "ROUND_HALF_DOWN"
            | "ROUND_HALF_EVEN"
            | "ROUND_HALF_UP"
            | "ROUND_UP"
            | "ROUND_05UP"
    )
    .then(|| Value::String(name.into()))
}
