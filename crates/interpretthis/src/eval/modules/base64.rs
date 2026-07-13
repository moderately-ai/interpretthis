// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `base64` module.
//!
//! Supports b64encode / b64decode and their urlsafe variants. Both
//! accept and return bytes — the CPython API. Encoding fails on
//! non-bytes input; decoding raises a clear ValueError on malformed
//! base64.

use base64::Engine as _;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::modules::value_error,
    value::Value,
};

pub fn has_function(name: &str) -> bool {
    matches!(name, "b64encode" | "b64decode" | "urlsafe_b64encode" | "urlsafe_b64decode")
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    let input = arg_bytes(func, args)?;
    let result = match func {
        "b64encode" => base64::engine::general_purpose::STANDARD.encode(&input).into_bytes(),
        "urlsafe_b64encode" => {
            base64::engine::general_purpose::URL_SAFE.encode(&input).into_bytes()
        }
        "b64decode" => base64::engine::general_purpose::STANDARD
            .decode(&input)
            .map_err(|e| value_error(format!("Invalid base64-encoded string: {e}")))?,
        "urlsafe_b64decode" => base64::engine::general_purpose::URL_SAFE
            .decode(&input)
            .map_err(|e| value_error(format!("Invalid base64-encoded string: {e}")))?,
        _ => {
            return Err(InterpreterError::AttributeError(format!(
                "module 'base64' has no attribute '{func}'"
            ))
            .into());
        }
    };
    Ok(Value::Bytes(result))
}

fn arg_bytes(func: &str, args: &[Value]) -> Result<Vec<u8>, EvalError> {
    let value = args.first().ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!("{func}() missing required argument")))
    })?;
    match value {
        Value::Bytes(b) => Ok(b.clone()),
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(InterpreterError::TypeError(format!(
            "{func}() requires bytes or str (got '{}')",
            other.type_name()
        ))
        .into()),
    }
}

/// `base64` module registration.
pub struct Base64Module;

#[async_trait::async_trait]
impl crate::eval::modules::Module for Base64Module {
    fn name(&self) -> &'static str {
        "base64"
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
