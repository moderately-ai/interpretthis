---
id: descriptor-precedence-nondata-vs-instance-dict
title: "Descriptor precedence: non-data vs data vs instance dict"
status: done
priority: p2
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [eval, eval/functions]
shared_scopes: []
paths: []
tags: [parity, descriptors, post-0.2]
---
## Problem

User descriptors with __get__/__set__/__delete__ and @property exist, but CPython
precedence (data descriptor > instance dict > non-data descriptor > __getattr__)
needs explicit tests and fixes for edge cases (class vs instance access, __set_name__ order).

## Acceptance

- Non-data descriptor (__get__ only) is shadowed by instance attribute assignment.
- Data descriptor (__set__ or __delete__) wins over instance dict.
- `C.x` vs `C().x` for descriptors matches CPython for pinned cases.
- Parity corpus covers both.

## Paths

`src/eval/names.rs`, `src/eval/classes.rs`, statements setattr, tests.
