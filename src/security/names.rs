// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

/// Names that are blocked from use in interpreter code.
pub const DANGEROUS_NAMES: &[&str] = &[
    "getattr",
    "setattr",
    "delattr",
    "eval",
    "exec",
    "compile",
    "__import__",
    "globals",
    "locals",
    "vars",
    "dir",
    "open",
    "file",
    "os",
    "sys",
    "subprocess",
    "shutil",
];

/// Attribute names that are blocked on all objects.
///
/// `__class__` is blocked alongside the rest of the class-walk chain
/// (`__bases__`, `__subclasses__`, `__mro__`, …). Classes *are* supported
/// in this sandbox; legitimate code should use `type(obj)` rather than
/// `obj.__class__`. Blocking the dunder removes the first step of
/// `().__class__.__bases__[0].__subclasses__()`-style probes.
pub const BLOCKED_ATTRIBUTES: &[&str] = &[
    "__class__",
    "__globals__",
    "__code__",
    "__closure__",
    "__dict__",
    "__subclasses__",
    "__bases__",
    "__mro__",
    "__builtins__",
    "__spec__",
    "__loader__",
];
