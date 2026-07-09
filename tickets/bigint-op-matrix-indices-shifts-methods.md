---
id: bigint-op-matrix-indices-shifts-methods
title: "BigInt op matrix: indices, shifts, methods, and overflow policy"
status: done
priority: p1
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [core, eval, eval/functions]
shared_scopes: []
paths: []
tags: [correctness, bigint, post-0.2]
---
## Problem

Hybrid `Int(i64)` / `BigInt` is shipped, but many paths still assume i64: slice/index
coercion, shifts, string/list repeat counts, and int methods (e.g. bit_length on huge ints).
Behavior is inconsistent (silent fail vs OverflowError vs TypeError).

## Acceptance

- Documented matrix: which ops promote, which require fitting i64/usize, exact errors.
- Index/slice/len-like uses raise clear OverflowError when out of range (CPython-aligned wording where possible).
- Shifts on BigInt are exact (not "return 0 if shift >= 64").
- Int methods either work on BigInt or raise a single consistent error.
- Parity or divergence tests pin the matrix.

## Paths

`src/value.rs`, `src/eval/operations.rs`, `src/types.rs`, method tables, place/subscript.
