---
id: slots-true-layout-no-dict-and-inheritance
title: "__slots__: true no-__dict__ semantics and slot inheritance"
status: todo
priority: p2
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [eval, core]
shared_scopes: []
paths: []
tags: [parity, classes, post-0.2]
---
## Problem

Current `__slots__` / dataclass `slots=True` is a field allowlist on SharedFields.
CPython: no instance `__dict__` unless configured, inheritance merges slots, weaker
memory layout. Hosts that rely on AttributeError for undeclared attrs partially work;
layout/identity divergences remain.

## Acceptance

- Documented model: either implement inheritance of slot names across bases, or CONFORMANCE permanent subset.
- `hasattr(instance, '__dict__')` / attribute access matches chosen model.
- Inheritance tests: child slots + parent slots.
- Prefer correctness over actual memory densification unless easy.

## Paths

`src/eval/classes.rs`, `src/value.rs` ClassValue, setattr paths, tests.
