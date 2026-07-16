// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal, sandbox-safe emulation of Python's `sys` module.
//!
//! Only the stable, non-environment-revealing surface is exposed:
//!   - `sys.stdout` / `sys.stderr` / `sys.stdin` — stream sentinels the
//!     `print` builtin recognises for its `file=` argument. `stdout`
//!     writes to the normal capture buffer; `stderr` is discarded from the
//!     captured stdout (matching CPython, where stderr is a separate stream).
//!   - `sys.maxsize` — the largest `Py_ssize_t` on a 64-bit build.
//!   - `sys.byteorder` — `"little"` (the only order we model).
//!
//! Deliberately NOT exposed (each would either leak host details that
//! diverge from the reference CPython or hand user code a capability the
//! sandbox withholds): `argv`, `path`, `platform`, `version`/`version_info`,
//! `modules`, `settrace`, `setrecursionlimit`, `executable`, `getsizeof`.
//! `sys.exit(code)` raises a catchable `SystemExit`, matching CPython.

use crate::{
    error::{EvalResult, InterpreterError},
    value::{ExceptionValue, Value},
};

/// The `sys.stdout` stream sentinel.
pub const STDOUT_SENTINEL: &str = "sys.stdout";
/// The `sys.stderr` stream sentinel.
pub const STDERR_SENTINEL: &str = "sys.stderr";

pub fn constant(name: &str) -> Option<Value> {
    match name {
        "stdout" => Some(Value::Type(STDOUT_SENTINEL.to_string())),
        "stderr" => Some(Value::Type(STDERR_SENTINEL.to_string())),
        "stdin" => Some(Value::Type("sys.stdin".to_string())),
        // Largest Py_ssize_t on a 64-bit CPython build.
        "maxsize" => Some(Value::Int(i64::MAX)),
        "byteorder" => Some(Value::String("little".into())),
        _ => None,
    }
}

pub fn has_function(name: &str) -> bool {
    matches!(name, "exit")
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        // `sys.exit([code])` raises SystemExit; `code` (default None) becomes
        // the exception's argument, as in CPython.
        "exit" => {
            let code = args.first().cloned().unwrap_or(Value::None);
            let message = match &code {
                Value::None => String::new(),
                Value::Int(n) => n.to_string(),
                other => format!("{other}"),
            };
            let mut exc = ExceptionValue::new("SystemExit", message);
            exc.args = vec![code];
            Err(crate::error::EvalError::Exception(exc))
        }
        _ => {
            Err(InterpreterError::AttributeError(format!("module 'sys' has no attribute '{func}'"))
                .into())
        }
    }
}

/// `sys` module registration.
pub struct SysModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for SysModule {
    fn name(&self) -> &'static str {
        "sys"
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
