// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Security policy for the Python AST interpreter.
//!
//! Threat model: adversarial or prompt-injected LLM output, not a general
//! multi-tenant OS sandbox. Headline mitigations:
//!
//! - **Import allowlist** — only modules registered in `eval/modules` may be
//!   imported; `__import__` is blocked by name. Dotted, relative, and star
//!   imports are rejected.
//! - **Dangerous names blocked** — `eval` / `exec` / `compile` / `getattr` /
//!   `setattr` / `delattr` / `globals` / `locals` / `vars` / `dir` / `open` /
//!   plus `os` / `sys` / `subprocess` / `shutil`. See [`names::DANGEROUS_NAMES`].
//!   (`input` is blocked separately as a builtin.)
//! - **Class-walk dunders blocked** — `__class__` / `__bases__` /
//!   `__subclasses__` / `__mro__` / `__globals__` / `__code__` / `__closure__` /
//!   `__dict__` / `__builtins__` / `__spec__` / `__loader__`. See
//!   [`names::BLOCKED_ATTRIBUTES`]. Single-underscore names (`obj._field`) are
//!   allowed.
//! - **DoS bounded** — memory, operation count, while iterations, recursion,
//!   stdout, cooperative wall-clock; collection/string multiply caps; checked
//!   integer arithmetic.
//!
//! Full attack → mitigation table: `THREAT_MODEL.md` at the crate root.
//!
//! - [`names`] — blocklists
//! - [`validator`] — enforcement helpers returning `InterpreterError::Security`

pub mod names;
pub mod validator;
