---
id: state-export-roundtrip-bigint-and-exception-groups
title: "State export/import: round-trip BigInt and ExceptionGroup.exceptions"
status: done
priority: p0
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [core]
shared_scopes: []
paths: []
tags: [correctness, state, post-0.2]
---
## Problem

0.2.0 added `Value::BigInt` and `ExceptionValue.exceptions`. Checkpoint format must
either round-trip them or fail loudly with a version/policy note. Silent loss is a bug.

## Acceptance

- Export then import preserves BigInt values and ExceptionGroup nested exceptions.
- If a field cannot be restored, document and version `STATE_FORMAT_VERSION` appropriately.
- Tests cover BigInt vars and a raised/caught ExceptionGroup in state.
- LruCache memo non-restore remains documented (existing policy OK).

## Paths

`src/serialize.rs`, `src/state.rs`, `src/value.rs`, state persistence tests.
