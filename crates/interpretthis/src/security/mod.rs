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
//! - **Dangerous names blocked** — `eval` / `exec` / `compile` / `__import__` /
//!   `globals` / `locals` / `dir` / `open` / `file` / `os` / `sys` /
//!   `subprocess` / `shutil`. See [`names::DANGEROUS_NAMES`]. (`input` is
//!   blocked separately as a builtin.)
//! - **Bounded builtins** — `getattr` / `setattr` / `delattr` are allowed but
//!   validate the attribute name against `BLOCKED_ATTRIBUTES`; `vars` is allowed
//!   but instance-only (returns a copy of an instance's fields — all already
//!   reachable via `getattr` — and rejects the no-arg / module / class forms
//!   that would re-expose scope bindings or the class-walk chain).
//! - **Class-walk dunders blocked** — `__bases__` / `__subclasses__` / `__mro__`
//!   / `__globals__` / `__code__` / `__closure__` / `__dict__` / `__builtins__` /
//!   `__spec__` / `__loader__` are blocked for both read and write. `__class__`
//!   is READ-allowed (it aliases `type(x)`, already reachable via the `type()`
//!   builtin, so it grants no capability) but WRITE-blocked (type confusion);
//!   with `__bases__`/`__mro__`/`__subclasses__` still gated, the class-walk
//!   escape dead-ends. See [`names::BLOCKED_ATTRIBUTES`]. Single-underscore
//!   names (`obj._field`) are allowed.
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
