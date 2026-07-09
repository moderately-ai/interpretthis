// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Stdlib module emulation.
//!
//! A small, security-reviewed subset of the Python standard library, one
//! submodule per emulated module. Every module implements the [`Module`]
//! trait — a single registration point ([`MODULES`]) maps the module name
//! to its handler so adding a stdlib module is one line in the registry
//! plus one trait impl in its own file. Modules carry no I/O, randomness,
//! or clock access, so everything here stays deterministic and
//! sandbox-safe.

pub mod base64;
pub mod collections;
pub mod copy_mod;
pub mod dataclasses;
pub mod datetime;
#[path = "decimal_mod.rs"]
pub mod decimal;
#[path = "enum_mod.rs"]
pub mod enum_mod;
pub mod fractions;
pub mod functools;
pub mod hashlib;
pub mod itertools;
pub mod json;
pub mod math;
pub mod re;
pub mod statistics;
pub mod string;
pub mod textwrap;
pub mod typing;

use std::{collections::HashMap, sync::LazyLock};

use async_trait::async_trait;
use indexmap::IndexMap;
use rustpython_parser::ast;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    tools::Tools,
    value::{ExceptionValue, Value},
};

/// A stdlib module the interpreter exposes to user code.
///
/// One unit struct per module file (`pub struct MathModule;`, etc.) with
/// an `impl Module for XModule` block carrying the module's surface.
/// Registration is a single line in [`MODULES`] — no per-module match arms
/// elsewhere. Default impls cover the common "no constants" / "no
/// functions" / "no callables" patterns so a constants-only module
/// (e.g. `string`) only overrides `constant` and `name`.
#[async_trait]
pub trait Module: Sync + Send {
    /// Module name as it appears in `import` statements.
    fn name(&self) -> &'static str;

    /// Lookup a module-level constant (`math.pi`); `None` otherwise.
    fn constant(&self, _name: &str) -> Option<Value> {
        None
    }

    /// Whether the module exposes `name` as a callable.
    fn has_function(&self, _name: &str) -> bool {
        false
    }

    /// Invoke `module.func(args, kwargs)`. The signature normalises the
    /// six historical shapes (some modules need only `(func, args)`,
    /// others need `state` for namedtuple / dataclass synthesis, others
    /// need `tools` for callback into user code via functools.reduce);
    /// modules ignore the inputs they don't use.
    async fn call(
        &self,
        state: &mut InterpreterState,
        func: &str,
        args: &[Value],
        kwargs: &IndexMap<String, Value>,
        tools: &Tools,
    ) -> EvalResult {
        let _ = (state, args, kwargs, tools);
        Err(InterpreterError::AttributeError(format!(
            "module '{}' has no callable '{func}'",
            self.name(),
        ))
        .into())
    }
}

/// The complete set of stdlib modules. Adding a module is one line here
/// plus one `pub struct XModule;` + `impl Module for XModule` in its
/// own file. Lookup is O(1) hashed by module name.
static MODULES: LazyLock<HashMap<&'static str, &'static dyn Module>> = LazyLock::new(|| {
    let modules: [&'static dyn Module; 18] = [
        &math::MathModule,
        &json::JsonModule,
        &re::ReModule,
        &datetime::DatetimeModule,
        &statistics::StatisticsModule,
        &collections::CollectionsModule,
        &string::StringModule,
        &textwrap::TextwrapModule,
        &base64::Base64Module,
        &hashlib::HashlibModule,
        &itertools::ItertoolsModule,
        &functools::FunctoolsModule,
        &typing::TypingModule,
        &enum_mod::EnumModule,
        &dataclasses::DataclassesModule,
        &decimal::DecimalModule,
        &fractions::FractionsModule,
        &copy_mod::CopyModule,
    ];
    modules.into_iter().map(|m| (m.name(), m)).collect()
});

/// Modules usable without an explicit import (resolved on bare-name lookup).
/// Mirrors the executor prompt's "auto-imported" set.
const AUTO_IMPORTED: &[&str] = &["json", "re", "datetime"];

/// Whether `name` is a module the interpreter can import.
#[must_use]
pub fn is_known_module(name: &str) -> bool {
    MODULES.contains_key(name)
}

/// Whether `name` resolves to a module without an explicit import.
#[must_use]
pub fn is_auto_imported(name: &str) -> bool {
    AUTO_IMPORTED.contains(&name)
}

/// Evaluate an `import a, b as c` statement.
pub fn eval_import(state: &mut InterpreterState, node: &ast::StmtImport) -> EvalResult {
    for alias in &node.names {
        let module = alias.name.as_str();
        if !is_known_module(module) {
            return Err(module_not_found(module));
        }
        // `import a.b` would bind `a`; only flat modules are supported, so a
        // dotted name is rejected rather than silently binding the wrong thing.
        if module.contains('.') {
            return Err(InterpreterError::Security(
                "dotted/submodule imports are not supported (see CONFORMANCE.md#import-allowlist)"
                    .into(),
            )
            .into());
        }
        let bind = alias.asname.as_ref().map_or(module, rustpython_parser::ast::Identifier::as_str);
        state
            .set_variable(bind, Value::Module(module.to_string()))
            .map_err(EvalError::Interpreter)?;
    }
    Ok(Value::None)
}

/// Evaluate a `from module import name, …` statement.
pub fn eval_import_from(state: &mut InterpreterState, node: &ast::StmtImportFrom) -> EvalResult {
    if node.level.is_some_and(|level| level.to_u32() > 0) {
        return Err(InterpreterError::Security(
            "relative imports are not supported (see CONFORMANCE.md#import-allowlist)".into(),
        )
        .into());
    }
    let module =
        node.module.as_ref().map(rustpython_parser::ast::Identifier::as_str).ok_or_else(|| {
            EvalError::from(InterpreterError::Security(
                "relative imports are not supported (see CONFORMANCE.md#import-allowlist)".into(),
            ))
        })?;
    if !is_known_module(module) {
        return Err(module_not_found(module));
    }
    for alias in &node.names {
        let name = alias.name.as_str();
        if name == "*" {
            return Err(InterpreterError::Security(
                "`from module import *` is not supported (see CONFORMANCE.md#import-allowlist)"
                    .into(),
            )
            .into());
        }
        let value = module_member(module, name)?;
        let bind = alias.asname.as_ref().map_or(name, rustpython_parser::ast::Identifier::as_str);
        state.set_variable(bind, value).map_err(EvalError::Interpreter)?;
    }
    Ok(Value::None)
}

/// Resolve `module.member` for attribute access and `from`-imports: a constant
/// returns its value; a function returns a callable [`Value::ModuleFunction`]
/// handle.
pub fn module_member(module: &str, name: &str) -> EvalResult {
    if let Some(value) = constant(module, name) {
        return Ok(value);
    }
    if has_function(module, name) {
        return Ok(Value::ModuleFunction { module: module.to_string(), name: name.to_string() });
    }
    Err(InterpreterError::AttributeError(format!("module '{module}' has no attribute '{name}'"))
        .into())
}

/// Invoke `module.func(args, kwargs)`. Routes through the [`MODULES`]
/// registry — O(1) hashed lookup on `module`, then dispatch through the
/// [`Module::call`] trait method. Async so modules that need to invoke
/// user-callable values (e.g. `functools.reduce(f, iter)` or
/// `itertools.takewhile(pred, iter)`) can call back into the evaluator.
pub async fn call_function(
    state: &mut crate::state::InterpreterState,
    module: &str,
    func: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &crate::tools::Tools,
) -> EvalResult {
    let handler = MODULES.get(module).ok_or_else(|| module_not_found(module))?;
    handler.call(state, func, args, kwargs, tools).await
}

fn constant(module: &str, name: &str) -> Option<Value> {
    MODULES.get(module)?.constant(name)
}

fn has_function(module: &str, name: &str) -> bool {
    MODULES.get(module).is_some_and(|m| m.has_function(name))
}

/// Resolve `Constructor.classmethod` when `Constructor` is a
/// [`Value::ModuleFunction`] (e.g. after `from datetime import datetime`).
///
/// CPython models these as type classmethods. Our constructors are flat
/// module functions, so `datetime.strptime(...)` arrives as a method-call
/// on a ModuleFunction receiver — see `eval_call` — and must be re-routed
/// to the underlying module function name returned here.
///
/// Returns `None` when the pair is not a known classmethod (caller raises
/// AttributeError).
#[must_use]
pub fn type_classmethod(module: &str, type_name: &str, method: &str) -> Option<&'static str> {
    match module {
        "datetime" => datetime::type_classmethod(type_name, method),
        _ => None,
    }
}

fn module_not_found(module: &str) -> EvalError {
    EvalError::Exception(ExceptionValue::new(
        "ModuleNotFoundError",
        format!("No module named '{module}'"),
    ))
}

// ---------------------------------------------------------------------------
// Shared argument helpers for module functions
// ---------------------------------------------------------------------------

/// The required positional argument at `index`, or a Python-style `TypeError`.
pub(crate) fn need_arg<'a>(
    func: &str,
    args: &'a [Value],
    index: usize,
) -> Result<&'a Value, EvalError> {
    args.get(index).ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!(
            "{func}() missing required argument at position {index}"
        )))
    })
}

/// A required numeric argument coerced to `f64` (accepts int/float/bool).
pub(crate) fn arg_f64(func: &str, args: &[Value], index: usize) -> Result<f64, EvalError> {
    need_arg(func, args, index)?.as_float().ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!(
            "{func}() expected a number at position {index}"
        )))
    })
}

/// A required string argument.
pub(crate) fn arg_str<'a>(
    func: &str,
    args: &'a [Value],
    index: usize,
) -> Result<&'a str, EvalError> {
    need_arg(func, args, index)?.as_str().ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!(
            "{func}() expected a string at position {index}"
        )))
    })
}

/// A `ValueError` `EvalError` with the given message.
pub(crate) fn value_error(message: impl Into<String>) -> EvalError {
    InterpreterError::ValueError(message.into()).into()
}

/// A `TypeError` `EvalError` with the given message. CPython raises
/// `TypeError` (not `ValueError`) for wrong-type arguments — most
/// notably `math.factorial(2.5)` and `math.isqrt(2.5)`, which both
/// want an integral argument.
pub(crate) fn type_error(message: impl Into<String>) -> EvalError {
    InterpreterError::TypeError(message.into()).into()
}

/// CPython's `OverflowError`, modelled as a `PythonException`
/// because the typed `InterpreterError` enum currently only carries
/// the most-common error types as native variants. Used for the
/// `cannot convert float infinity to integer` case — CPython
/// distinguishes between OverflowError (infinity) and ValueError
/// (NaN) for float→int conversions.
pub(crate) fn overflow_error(message: impl Into<String>) -> EvalError {
    typed_exception("OverflowError", message)
}

/// `statistics.StatisticsError`, raised by the statistics module.
/// CPython subclasses ValueError but the rendered type name still
/// reads `statistics.StatisticsError`, so a planner LLM only sees
/// the right wording if we surface that qualified name.
pub(crate) fn statistics_error(message: impl Into<String>) -> EvalError {
    typed_exception("statistics.StatisticsError", message)
}

/// `json.decoder.JSONDecodeError`, raised by `json.loads` on invalid
/// input. Subclass of `ValueError` in CPython; the str(e) form uses
/// the qualified subclass name.
pub(crate) fn json_decode_error(message: impl Into<String>) -> EvalError {
    typed_exception("json.decoder.JSONDecodeError", message)
}

fn typed_exception(type_name: &str, message: impl Into<String>) -> EvalError {
    ExceptionValue::new(type_name, message).into()
}

/// Invoke a user-provided callable with the given args. Shared by
/// stdlib modules that take a callback (functools.reduce,
/// itertools.takewhile / dropwhile / accumulate, etc.).
///
/// Delegates to `call_value_as_function` so every callable shape the
/// interpreter understands (Function, Lambda, BoundMethod (snapshot
/// and place), BuiltinTypeMethod, ModuleFunction, and the
/// `__builtin__`/`__tool__`/`__class_method__` sentinel strings) is
/// dispatched through one table. kwargs are dropped — the only
/// callers (itertools predicates, functools.reduce binary fn) pass
/// empty kwargs anyway, and broadcasting them through the sentinel
/// paths would require routing every dispatch through the same
/// kwarg-aware machinery which doesn't pay off for the binary-fn
/// use case.
pub(crate) async fn call_callable(
    state: &mut InterpreterState,
    callable: &Value,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &crate::tools::Tools,
) -> EvalResult {
    let _ = kwargs;
    crate::eval::functions::call_value_as_function(state, callable, args, tools).await
}
