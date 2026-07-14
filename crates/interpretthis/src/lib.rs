// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Sandboxed Python AST interpreter with host tool injection and resource limits.
//!
//! Evaluates [`rustpython_parser`] ASTs — not an embedded CPython. There is no
//! filesystem, network, or process access unless the host registers tools that
//! provide them.
//!
//! # Quick start
//!
//! ```no_run
//! use std::collections::HashMap;
//!
//! use interpretthis::{
//!     Interpreter, InterpreterConfig, InterpreterDeps, KwargsExt, ToolDefinition, Tools, Value,
//! };
//!
//! # async fn demo() {
//! let tools = Tools::new().with(
//!     "double",
//!     ToolDefinition::from_fn(|kwargs| async move {
//!         let n = kwargs.require_int("n")?;
//!         Ok(Value::Int(n * 2))
//!     }),
//! );
//! let interp = Interpreter::new(
//!     InterpreterDeps { tools },
//!     InterpreterConfig::default(),
//! );
//! let resp = interp
//!     .execute(
//!         "result = double(n=x)\nprint(result)",
//!         &Tools::new(),
//!         HashMap::from([("x".to_string(), Value::Int(42))]),
//!     )
//!     .await;
//! assert!(resp.is_ok());
//! # }
//! ```
//!
//! Registered tools on [`InterpreterDeps`] and the per-call `tools` argument are
//! merged; on a name clash the per-call tool wins.
//!
//! # Contracts worth knowing
//!
//! - **Tool errors** surface as [`InterpreterError::Tool`]. Uncaught, they fail
//!   the host `execute` call; inside user Python they become a generic
//!   `Exception` and **can** be caught by bare `except` / `except Exception`.
//! - **State export** ([`Interpreter::export_state`]) is a **versioned byte
//!   blob** (4-byte little-endian `STATE_FORMAT_VERSION` + JSON body).
//!   Mismatched versions fail with [`InterpreterError::StateFormatSuperseded`].
//!   Lazy tool proxies are omitted. Signing is a host concern.
//! - **Language surface** is intentional, not accidental. Divergences and the
//!   stdlib allowlist live in the repo’s `CONFORMANCE.md`; the security boundary
//!   is described in `THREAT_MODEL.md`.
//! - **Integers** use a hybrid representation: values that fit in `i64` stay
//!   compact; larger results promote automatically (CPython-like arbitrary
//!   precision). Extremely large powers/shifts are resource-capped.
//! - **ExceptionGroup** / `except*` (PEP 654 leaf split) are available; nested
//!   group APIs are still incomplete — see CONFORMANCE.
//! - **async/await** is not supported; host code should await around
//!   [`Interpreter::execute`] instead.

pub mod config;
pub mod error;
pub mod interpreter;
pub mod tools;
pub mod value;

pub(crate) mod eval;
pub(crate) mod parser;
pub(crate) mod security;
pub(crate) mod serialize;
pub(crate) mod state;
pub(crate) mod types;

// --- Public re-exports: core types ---
pub use config::InterpreterConfig;
pub use error::InterpreterError;
pub use interpreter::{Interpreter, InterpreterDeps, InterpreterResponse};
// Wire-format version for state checkpoints (export/import).
pub use serialize::STATE_FORMAT_VERSION;
// --- Public re-exports: tool system ---
pub use tools::{KwargsExt, ToolDefinition, ToolError, ToolHandler, Tools};
// Host-facing value surface. Deeper interpreter shapes (`ClassValue`,
// `FunctionDef`, `MatchValue`, …) remain available under `interpretthis::value`
// for advanced hosts but are intentionally not re-exported at the crate root.
pub use value::{ExceptionValue, Value, ValueKey, shared_bytes, shared_list};
