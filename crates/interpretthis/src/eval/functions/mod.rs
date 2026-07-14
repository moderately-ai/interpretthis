// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

#![expect(
    clippy::cast_precision_loss,
    reason = "Python built-ins (sum, min, max, `**`, int(float)) convert Int ↔ \
              Float; precision loss above 2^53 is CPython's behaviour and we \
              faithfully reproduce it. Scoped to this module since the numeric \
              built-ins are here"
)]

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
    VariableCheckpoint, collect_assigned_names, contains_yield_stmts, extract_function_source,
};
// Re-exports from definitions
pub use definitions::{build_function_params, eval_function_def, eval_lambda_def};
// Re-exports from dispatch
pub(crate) use dispatch::{call_lambda, call_user_function, call_value_as_function};
// Re-exports from method_dispatch
pub(crate) use generators::dispatch_generator_method;
pub(crate) use method_dispatch::{
    CallArgs, MethodOutcome, arg1, bind_method_params, reject_kwargs, require_param,
};
pub(crate) use params::{bind_params, evaluate_param_defaults, execute_body};

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
        Value::Float(_) => Err(InterpreterError::TypeError(
            "'float' object cannot be interpreted as an integer".into(),
        )
        .into()),
        _ => {
            Err(InterpreterError::TypeError(format!("expected integer, got '{}'", val.type_name()))
                .into())
        }
    }
}
