---
id: decimal-prec-per-interpreter-not-process-global
title: Decimal prec must be per-InterpreterState (drop process AtomicI64)
status: ready
priority: p0
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [eval/modules, core]
shared_scopes: []
paths: []
tags: [correctness, decimal, post-0.2]
---
## Problem

`decimal` active precision is mirrored in a process-global `AtomicI64` (`DECIMAL_PREC`)
while also living on `InterpreterState.decimal_prec`. Two concurrent interpreters in one
process can clobber each other's division precision.

## Acceptance

- Division / context reads use only `InterpreterState` (or a handle derived from it).
- No process-global atomic for prec.
- Two interpreters with different `getcontext().prec` do not interfere (test).
- CONFORMANCE note updated if any residual divergence remains.

## Paths

`src/eval/modules/decimal_mod.rs`, `src/state.rs`, `src/types.rs` (decimal arith), tests.
