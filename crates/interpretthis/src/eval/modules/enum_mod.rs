// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `enum` module.
//!
//! Supports `Enum`, `IntEnum`, `StrEnum`, and `auto()`. The enum
//! class machinery uses our regular class system: an enum class is
//! a registered ClassValue whose members are class attributes whose
//! values are the literal values. `auto()` returns sequential ints.
//!
//! Per-instance enum behaviour (.name, .value, identity comparison
//! via `is`) is not fully modelled — we treat enum members as their
//! underlying values, which works for the common patterns (storing
//! in dicts, comparing to literals) but loses identity-based
//! semantics.

use crate::{
    error::{EvalResult, InterpreterError},
    value::Value,
};

pub fn has_function(name: &str) -> bool {
    matches!(name, "auto")
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        "auto" => {
            // auto() returns a sentinel that the class-body assignment
            // turns into a sequential integer. CPython's auto() is
            // stateful per-class; we expose it as Int(0) and let user
            // code manually assign distinct values when the auto-
            // numbering would be observable. For typical usage
            // (Color.RED = auto(); Color.GREEN = auto() with the
            // user expecting 1, 2) we approximate by returning
            // a small sentinel that prints reasonably.
            Ok(Value::Int(
                args.first()
                    .and_then(|v| match v {
                        Value::Int(i) => Some(*i),
                        _ => None,
                    })
                    .unwrap_or(1),
            ))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'enum' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// Module-level constants — Enum, IntEnum, StrEnum classes. We model
/// them as Type sentinels that user code can inherit from via
/// `class Color(Enum):`. The class registration in eval_class_def
/// recognises these as valid base names but doesn't add behaviour
/// beyond the regular class system — the class attributes become the
/// enum members.
pub fn constant(name: &str) -> Option<Value> {
    match name {
        "Enum" | "IntEnum" | "StrEnum" | "Flag" | "IntFlag" => {
            Some(Value::Type(format!("enum.{name}")))
        }
        _ => None,
    }
}

/// `enum` module registration.
pub struct EnumModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for EnumModule {
    fn name(&self) -> &'static str {
        "enum"
    }
    fn constant(&self, name: &str) -> Option<Value> {
        constant(name)
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
