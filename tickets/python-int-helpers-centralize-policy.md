---
id: python-int-helpers-centralize-policy
title: Centralize Python-int helpers (as_bigint/as_i64/from_bigint/to_int policy)
status: todo
priority: p2
dependencies: []
related: [epic-post-0-2-hardening-and-parity, bigint-op-matrix-indices-shifts-methods]
scopes: [core, eval]
shared_scopes: []
paths: []
tags: [hygiene, refactor, post-0.2]
---
## Problem

`value_as_bigint`, `value_as_i64`, `int_from_bigint`, `to_int`, `to_bigint` are scattered
across operations, types, builtins. Policies for "must fit i64" diverge.

## Acceptance

- Single module (e.g. `src/int_value.rs` or `value::int_ops`) owns conversion policy.
- Call sites use helpers; no ad-hoc BigInt matching in arith without reason.
- Document size/index vs pure arithmetic distinction.

## Paths

`src/value.rs`, `src/eval/operations.rs`, `src/types.rs`, builtins.
