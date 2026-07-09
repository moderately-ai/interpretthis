// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::value::ExceptionValue;

/// Errors surfaced to the host (consumer of the interpreter).
///
/// All error variants carry a human-readable message. The variant determines
/// the category of failure. Use pattern matching or `Display` for error reporting.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum InterpreterError {
    /// CPython-shaped prefix: `SyntaxError: <msg>`.
    #[error("SyntaxError: {0}")]
    Syntax(String),

    /// Sandbox-specific (no CPython equivalent — CPython doesn't have
    /// a generic "security" rejection). Kept as-is so the prefix is
    /// recognisable as sandbox enforcement rather than a
    /// language-level error.
    #[error("SecurityError: {0}")]
    Security(String),

    /// CPython prefix: `RuntimeError: <msg>`.
    #[error("RuntimeError: {0}")]
    Runtime(String),

    /// Sandbox-specific (CPython has no operation-counter limit).
    /// Kept as-is.
    #[error("LimitExceeded: {0}")]
    LimitExceeded(String),

    /// CPython: `RecursionError: maximum recursion depth exceeded`.
    /// Distinct from `LimitExceeded` so callers can recognise the
    /// Python-equivalent and hint at `sys.setrecursionlimit`-style
    /// mitigations instead of memory exhaustion.
    #[error("RecursionError: maximum recursion depth exceeded ({limit})")]
    RecursionLimitExceeded {
        /// The configured `max_recursion_depth` that was breached.
        limit: u32,
    },

    /// Sandbox-specific (CPython has no tool concept).
    #[error("ToolError in '{tool_name}': {message}")]
    Tool { tool_name: String, message: String },

    /// CPython prefix: `NameError: <message>`. The inner String is
    /// the full message body (typically `name 'X' is not defined`),
    /// NOT just the variable name — that lets `stamp_line` append a
    /// line marker at the end of the rendered output instead of
    /// inside the quoted name. Use [`Self::name_not_defined`] to
    /// construct the common form.
    #[error("NameError: {0}")]
    NameError(String),

    /// CPython prefix: `TypeError: <msg>`.
    #[error("TypeError: {0}")]
    TypeError(String),

    /// CPython prefix: `ValueError: <msg>`.
    #[error("ValueError: {0}")]
    ValueError(String),

    /// CPython prefix: `AttributeError: <msg>`.
    #[error("AttributeError: {0}")]
    AttributeError(String),

    /// CPython prefix: `AssertionError: <msg>` (or `AssertionError`
    /// alone when the assert had no message). Inner String includes
    /// the message verbatim — empty when none.
    #[error("AssertionError: {0}")]
    AssertionError(String),

    /// Raised-from-Python exceptions: `<type_name>: <message>`. Already
    /// CPython-shape since the type_name is the exception class.
    #[error("{type_name}: {message}")]
    PythonException { type_name: String, message: String },

    /// Imported state blob carries a `STATE_FORMAT_VERSION` the current
    /// interpreter no longer understands. The interpreter never silently
    /// migrates across format versions — pre-versioning blobs (no version
    /// prefix) are reported as `found = 0`, and any other mismatch surfaces
    /// the exact version found vs the version expected.
    ///
    /// Do not feed the blob into a newer interpreter shape; restart the
    /// host workflow from a clean initial state instead.
    #[error(
        "interpreter state format superseded: found version {found}, expected {expected}; \
         host must restart from initial step"
    )]
    StateFormatSuperseded {
        /// Version embedded in the imported blob (0 for pre-versioning
        /// blobs that have no version prefix).
        found: u32,
        /// Version this interpreter writes and accepts.
        expected: u32,
    },
}

impl InterpreterError {
    /// Build the canonical `name 'X' is not defined` NameError body.
    /// The inner String is the FULL message (matches CPython's
    /// `NameError: name '<n>' is not defined`); callers that produce
    /// the rare other-body NameError can wrap their own String
    /// directly in `Self::NameError(...)`.
    #[must_use]
    pub fn name_not_defined(name: impl AsRef<str>) -> Self {
        Self::NameError(format!("name '{}' is not defined", name.as_ref()))
    }
}

/// Internal control flow signals — not visible to user code.
#[derive(Debug, Clone)]
pub(crate) enum ControlFlow {
    Break,
    Continue,
    Return(Box<crate::value::Value>),
}

/// Internal error type used throughout the evaluator layer.
/// Distinguishes real errors, control flow signals, and Python exceptions.
#[derive(Debug, Clone)]
pub(crate) enum EvalError {
    /// Fatal interpreter errors — not catchable by user code.
    Interpreter(InterpreterError),
    /// Control flow signals — caught by loops/functions, NOT by try/except.
    Signal(ControlFlow),
    /// Python-level exceptions — caught by try/except.
    Exception(ExceptionValue),
}

impl From<InterpreterError> for EvalError {
    fn from(e: InterpreterError) -> Self {
        Self::Interpreter(e)
    }
}

impl From<ExceptionValue> for EvalError {
    fn from(e: ExceptionValue) -> Self {
        Self::Exception(e)
    }
}

/// Result type used by all evaluators.
pub(crate) type EvalResult = Result<crate::value::Value, EvalError>;
