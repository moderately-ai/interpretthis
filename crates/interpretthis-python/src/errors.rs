// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `InterpreterError` -> Python exception mapping.
//!
//! The exception *classes* are defined in Python (`python/interpretthis/
//! _exceptions.py`) and imported here, rather than being created with pyo3's
//! `create_exception!`. That is what lets each class subclass the builtin it
//! mirrors — `SandboxNameError` is both an `InterpretThisError` and a real
//! `NameError` — so a caller's `except NameError:` behaves the way the name
//! promises. A Rust-side `create_exception!` can only inherit from one base
//! that Rust names, and it cannot carry Python-level behaviour.
//!
//! Errors raised out of sandboxed code are *not* the host's errors: they are
//! data about a failed run. `execute()` therefore returns them inside an
//! `ExecutionResult` rather than raising (see `lib.rs`); these classes exist for
//! `ExecutionResult.check()` and for the registration paths that genuinely do
//! fail the caller.

use interpretthis::InterpreterError;
use pyo3::{import_exception, prelude::*};

import_exception!(interpretthis, InterpretThisError);
import_exception!(interpretthis, SandboxSyntaxError);
import_exception!(interpretthis, SecurityError);
import_exception!(interpretthis, SandboxRuntimeError);
import_exception!(interpretthis, LimitExceededError);
import_exception!(interpretthis, RecursionLimitError);
import_exception!(interpretthis, ToolError);
import_exception!(interpretthis, SandboxNameError);
import_exception!(interpretthis, SandboxTypeError);
import_exception!(interpretthis, SandboxValueError);
import_exception!(interpretthis, SandboxAttributeError);
import_exception!(interpretthis, SandboxAssertionError);
import_exception!(interpretthis, PythonException);
import_exception!(interpretthis, StateFormatError);

/// Build the Python exception mirroring an [`InterpreterError`].
///
/// Every class takes `(message, ...)` positionally; the extra fields are the
/// structured payload a caller would otherwise have to re-parse out of the
/// message string (`ToolError.tool_name`, `PythonException.type_name`, ...).
pub fn to_pyerr(err: &InterpreterError) -> PyErr {
    let message = err.to_string();

    match err {
        InterpreterError::Syntax(_) => SandboxSyntaxError::new_err(message),
        InterpreterError::Security(_) => SecurityError::new_err(message),
        InterpreterError::Runtime(_) => SandboxRuntimeError::new_err(message),
        InterpreterError::LimitExceeded(_) => LimitExceededError::new_err(message),
        InterpreterError::RecursionLimitExceeded { limit } => {
            RecursionLimitError::new_err((message, *limit))
        }
        InterpreterError::Tool { tool_name, .. } => {
            ToolError::new_err((message, tool_name.clone()))
        }
        InterpreterError::NameError(_) => SandboxNameError::new_err(message),
        InterpreterError::TypeError(_) => SandboxTypeError::new_err(message),
        InterpreterError::ValueError(_) => SandboxValueError::new_err(message),
        InterpreterError::AttributeError(_) => SandboxAttributeError::new_err(message),
        InterpreterError::AssertionError(_) => SandboxAssertionError::new_err(message),
        InterpreterError::PythonException { type_name, .. } => {
            PythonException::new_err((message, type_name.clone()))
        }
        InterpreterError::StateFormatSuperseded { found, expected } => {
            StateFormatError::new_err((message, *found, *expected))
        }
        // InterpreterError is #[non_exhaustive]: a variant added upstream lands
        // on the base class with its Display text rather than vanishing.
        _ => InterpretThisError::new_err(message),
    }
}
