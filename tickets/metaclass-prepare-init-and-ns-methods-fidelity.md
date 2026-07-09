---
id: metaclass-prepare-init-and-ns-methods-fidelity
title: "Metaclass: __prepare__, __init__, and methods surviving type() rebuild"
status: done
priority: p1
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [eval, eval/functions]
shared_scopes: []
paths: []
tags: [parity, classes, post-0.2]
---
## Problem

`metaclass=` + `__new__` and three-arg `type(name, bases, dict)` work for simple cases.
Gaps: `__prepare__`, metaclass `__init__` after create, methods defined in class body
that live in `ClassValue.methods` not only namespace dict when Meta rebuilds via `type()`.

## Acceptance

- If Meta defines `__prepare__`, use its mapping as the class namespace seed.
- After successful create, call Meta.__init__(cls, name, bases, ns) when present.
- Class body `def` methods remain callable on instances after a Meta.__new__ that returns `type(...)`.
- Document remaining permanent gaps (cooperative multi-metaclass) in CONFORMANCE.

## Paths

`src/eval/classes.rs`, function body registration, tests under classes/.
