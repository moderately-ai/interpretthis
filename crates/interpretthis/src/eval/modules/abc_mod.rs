// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `abc` module (abstract base classes).
//!
//! `ABC`/`ABCMeta` are exposed as `Value::Type` base sentinels (like
//! `enum.Enum`): `class Shape(ABC): ...` registers a normal `ClassValue`,
//! and `eval_class_def` skips the sentinel base. `@abstractmethod` is
//! recognised syntactically at class-definition time (it does not need to run),
//! which records the method in the class's abstract set; `instantiate` then
//! refuses a class that still has unimplemented abstract methods, matching
//! CPython's `TypeError: Can't instantiate abstract class …`.

use crate::{error::EvalResult, value::Value};

/// The abstract-decorator names recognised on a method (`@abstractmethod`,
/// `@abc.abstractmethod`, and the deprecated `abstractproperty` family).
pub const ABSTRACT_DECORATORS: &[&str] =
    &["abstractmethod", "abstractproperty", "abstractclassmethod", "abstractstaticmethod"];

pub fn has_function(name: &str) -> bool {
    ABSTRACT_DECORATORS.contains(&name)
}

/// `ABC`/`ABCMeta` base sentinels; `eval_class_def` recognises these.
pub fn constant(name: &str) -> Option<Value> {
    match name {
        "ABC" | "ABCMeta" => Some(Value::Type(format!("abc.{name}"))),
        _ => None,
    }
}

/// The abstract decorators are identity functions at runtime — the method is
/// flagged syntactically during class construction, so a stray runtime call
/// (`abc.abstractmethod(f)`) just returns `f`.
pub fn call(func: &str, args: &[Value]) -> EvalResult {
    if ABSTRACT_DECORATORS.contains(&func) {
        return Ok(args.first().cloned().unwrap_or(Value::None));
    }
    Err(crate::error::InterpreterError::AttributeError(format!(
        "module 'abc' has no attribute '{func}'"
    ))
    .into())
}

/// `abc` module registration.
pub struct AbcModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for AbcModule {
    fn name(&self) -> &'static str {
        "abc"
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
