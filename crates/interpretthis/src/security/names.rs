// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

/// Names that are blocked from use in interpreter code.
pub const DANGEROUS_NAMES: &[&str] = &[
    // getattr/setattr/delattr — and vars — are available as *bounded* builtins:
    // the attribute name is validated against BLOCKED_ATTRIBUTES (see
    // CONFORMANCE.md#eval-exec). Bare names stay off this list so the builtins
    // can resolve. `vars` accepts ONLY an instance (returning a copy of its
    // fields, which are all already reachable via getattr and provably free of
    // blocked-dunder keys); the module/class/no-arg forms that would re-expose
    // scope bindings or the class-walk chain (bases/mro) raise TypeError.
    "eval",
    "exec",
    "compile",
    "__import__",
    "globals",
    "locals",
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
