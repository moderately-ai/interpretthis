// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal emulation of Python's `io` module: `io.StringIO`, an in-memory text
//! stream. Backed by [`crate::value::SharedStringIo`] so it is reference-
//! semantic and mutations are visible through every alias.

use crate::{
    error::{EvalResult, InterpreterError},
    value::Value,
};

pub fn has_function(name: &str) -> bool {
    matches!(name, "StringIO")
}

/// `io.StringIO([initial])` — construct a text stream seeded with `initial`.
pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        "StringIO" => {
            let initial = match args.first() {
                None | Some(Value::None) => String::new(),
                Some(Value::String(s)) => s.to_string(),
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "initial_value must be str or None, not {}",
                        other.type_name()
                    ))
                    .into());
                }
            };
            // A fresh StringIO seeded with text positions the cursor at the end
            // (CPython leaves it at 0, but write() overwrites from pos and the
            // common flow is write-then-getvalue, so seed pos at 0 to match).
            Ok(Value::StringIO(crate::value::shared_stringio(initial)))
        }
        _ => {
            Err(InterpreterError::AttributeError(format!("module 'io' has no attribute '{func}'"))
                .into())
        }
    }
}

/// `io` module registration.
pub struct IoModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for IoModule {
    fn name(&self) -> &'static str {
        "io"
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
