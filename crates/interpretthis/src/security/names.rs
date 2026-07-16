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

/// Attribute names gated on all objects.
///
/// Every name here is rejected on **write** (`obj.attr = …`, `setattr`,
/// `delattr`, `__setattr__`) — the single funnel is
/// [`validate_attribute`](crate::security::validator::validate_attribute), which
/// every mutation site calls. On **read**, all of them are blocked too, EXCEPT
/// `__class__`: it aliases `type(x)`, which is already reachable via the
/// `type()` builtin, so reading it grants no capability the caller lacks. The
/// read alias is resolved by `crate::eval::names::resolve_object_attr` (a
/// universal object-level attribute) *before* the read-side validator runs;
/// `__class__` stays in this list purely so *assigning* it — in-sandbox type
/// confusion — remains blocked at every write site.
///
/// The remaining entries are the class-walk escape chain (`__bases__`,
/// `__mro__`, `__subclasses__`) and interpreter internals (`__globals__`,
/// `__code__`, `__closure__`, `__dict__`, …); reading any of them would hand
/// sandboxed code the object graph or the interpreter's own state, so they are
/// blocked in both directions. This is what severs
/// `().__class__.__bases__[0].__subclasses__()`-style probes: even though
/// `().__class__` now resolves (to `tuple`), `__bases__`/`__mro__`/
/// `__subclasses__` stay blocked, so the walk dead-ends immediately.
pub const BLOCKED_ATTRIBUTES: &[&str] = &[
    // Read-allowed (aliases `type(x)`), write-blocked. See the note above.
    "__class__",
    // Blocked in both directions — escape chain + interpreter internals.
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
