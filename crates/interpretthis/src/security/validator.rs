// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Centralised security checks for names and attributes.
//!
//! Previously the `DANGEROUS_NAMES` and `BLOCKED_ATTRIBUTES` constants lived in
//! `security::names` but every callsite ran its own `.contains(...)` check and
//! built its own error message. Consolidate the checks here so every guard
//! path returns a uniform `InterpreterError::Security` shape and the security
//! policy is auditable from one file.

use crate::{
    error::{EvalError, InterpreterError},
    security::names::{BLOCKED_ATTRIBUTES, DANGEROUS_NAMES},
};

/// Identifies *where* a dangerous-name check is being applied, so the error
/// surface can distinguish "you tried to read a dangerous name" from
/// "you tried to redefine one via `def` / assignment".
///
/// The message emitted by [`validate_name`] branches on this so callers
/// don't each hand-roll their own `format!(...)` with their own wording —
/// this keeps messages consistent when audit tools grep for them.
///
/// State-restore paths (`serialize::import_state`) do not use this enum:
/// they raise `InterpreterError::Security` directly (not wrapped in
/// `EvalError`), so they use [`is_name_allowed`] as the raw predicate and
/// build their own message.
#[derive(Debug, Clone, Copy)]
pub enum NameContext {
    /// Accessing / reading a name (variable lookup).
    Access,
    /// Defining a function with this name (`def name(…):`).
    FunctionDefinition,
    /// Assigning to this name (`name = …`).
    Assignment,
}

/// Fail with a security error if `name` appears in [`DANGEROUS_NAMES`].
///
/// The `ctx` parameter controls the message wording so the caller surface
/// mirrors the interpreter's error taxonomy without each call site building
/// its own `format!(...)`.
pub fn validate_name(ctx: NameContext, name: &str) -> Result<(), EvalError> {
    if DANGEROUS_NAMES.contains(&name) {
        let msg = match ctx {
            NameContext::Access => format!("access to '{name}' is not allowed"),
            NameContext::FunctionDefinition => {
                format!("function name '{name}' is a dangerous builtin and not allowed")
            }
            NameContext::Assignment => format!("cannot assign to dangerous name '{name}'"),
        };
        return Err(InterpreterError::Security(msg).into());
    }
    Ok(())
}

/// Pure query: is `name` safe to expose (e.g. as a tool)?
///
/// Non-fatal inverse of [`validate_name`]; used by the tool-registration
/// assert which cannot construct an `EvalError`.
#[must_use]
pub fn is_name_allowed(name: &str) -> bool {
    !DANGEROUS_NAMES.contains(&name)
}

/// Fail with a security error if `attr_name` appears in
/// [`BLOCKED_ATTRIBUTES`].
///
/// Called from every `obj.attr` evaluation path so the policy is single-
/// sourced. Previously this also blocked any attribute beginning with a
/// single underscore (`_private`) as a defence-in-depth measure, but in
/// Python `_attr` is a NAMING CONVENTION (a hint that the attribute is
/// internal), not a security boundary — `obj._field` access is allowed
/// freely in CPython and is idiomatic for any class with a property
/// backed by `_field`. Forbidding it broke every customer class that
/// followed the convention. The genuinely dangerous attributes
/// (`__class__`, `__globals__`, `__code__`, `__bases__`, `__mro__`, …)
/// that can be used to walk to interpreter internals are enumerated
/// explicitly in `BLOCKED_ATTRIBUTES`.
pub fn validate_attribute(attr_name: &str) -> Result<(), EvalError> {
    if BLOCKED_ATTRIBUTES.contains(&attr_name) {
        return Err(InterpreterError::Security(format!(
            "access to '{attr_name}' is not permitted for security reasons"
        ))
        .into());
    }

    Ok(())
}
