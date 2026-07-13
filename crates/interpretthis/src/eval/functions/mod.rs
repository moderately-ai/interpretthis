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

/// Convert a non-negative `i64` exponent or shift count (range-checked by
/// the caller) into a `u32`. Returns a runtime error on invariant violation.
pub(crate) fn to_u32(n: i64) -> Result<u32, EvalError> {
    u32::try_from(n).map_err(|_| {
        InterpreterError::Runtime(
            "exponent/shift count out of u32 range (internal invariant)".into(),
        )
        .into()
    })
}

/// Python's `int(float)` truncates toward zero and saturates at `i64`
/// bounds. NaN maps to 0 (Python raises `ValueError`; this is more lenient,
/// matching the current interpreter's behavior).
#[expect(
    clippy::cast_possible_truncation,
    reason = "truncation toward zero IS Python's int(float) semantic; the \
              saturation branches above handle out-of-range inputs"
)]
pub(crate) fn float_to_int(f: f64) -> i64 {
    if f.is_nan() {
        0
    } else if f >= i64::MAX as f64 {
        i64::MAX
    } else if f <= i64::MIN as f64 {
        i64::MIN
    } else {
        f.trunc() as i64
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

pub(crate) fn value_to_i64(val: &Value) -> Result<i64, EvalError> {
    match val {
        Value::Int(i) => Ok(*i),
        // Python's int(float) truncates toward 0 and saturates at i64
        // bounds (see float_to_int for the semantic).
        Value::Float(f) => Ok(float_to_int(*f)),
        Value::Bool(b) => Ok(i64::from(*b)),
        _ => {
            Err(InterpreterError::TypeError(format!("expected integer, got '{}'", val.type_name()))
                .into())
        }
    }
}
