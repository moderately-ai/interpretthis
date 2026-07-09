---
id: bigint-resource-limits-digits-bits-op-cost
title: "BigInt resource limits: max digits/bits and op-cost scaling"
status: todo
priority: p1
dependencies: []
related: [epic-post-0-2-hardening-and-parity, bigint-op-matrix-indices-shifts-methods]
scopes: [core, security, eval]
shared_scopes: []
paths: []
tags: [security, bigint, post-0.2]
---
## Problem

Power exponents are capped (~1e6), but huge ints can still be built via other ops and
counted as cheap single operations. Memory estimates for BigInt are approximate.
Sandbox hosts need bit/digit budgets.

## Acceptance

- Configurable or hard limits for max bit length / decimal digits (config or security constants).
- Op counter or memory accounting scales with bit length for mul/pow/shift (documented formula).
- Exceeding limits raises a clear host/runtime error before OOM.
- Tests for limit enforcement without flaking CI memory.

## Paths

`src/config.rs`, `src/state.rs` estimate_value_size, arith paths, security tests.
