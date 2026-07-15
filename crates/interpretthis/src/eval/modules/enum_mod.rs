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

/// The sentinel value `auto()` returns, replaced during enum class
/// construction. Modelled as an otherwise-unused `ModuleFunction` handle so it
/// is distinct from any real member value.
#[must_use]
pub fn auto_sentinel() -> Value {
    Value::ModuleFunction { module: "enum".into(), name: "__auto__".into() }
}

/// Whether `value` is the [`auto_sentinel`].
#[must_use]
pub fn is_auto_sentinel(value: &Value) -> bool {
    matches!(value, Value::ModuleFunction { module, name } if module == "enum" && name == "__auto__")
}

pub fn call(func: &str, _args: &[Value]) -> EvalResult {
    match func {
        "auto" => {
            // auto() returns a distinct sentinel that the enum class builder
            // (`wrap_enum_member`) replaces with the next sequential value:
            // highest previous value + 1 for int-valued enums, or the
            // lowercased member name for a StrEnum (CPython semantics). The
            // sentinel must be distinguishable from an explicit `= 1`.
            Ok(auto_sentinel())
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
