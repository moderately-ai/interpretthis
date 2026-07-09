---
id: decimal-context-rounding-traps-localcontext
title: "Decimal context: rounding modes, traps, localcontext"
status: todo
priority: p2
dependencies: [decimal-prec-per-interpreter-not-process-global]
related: [epic-post-0-2-hardening-and-parity, decimal-prec-per-interpreter-not-process-global]
scopes: [eval/modules]
shared_scopes: []
paths: []
tags: [parity, decimal, post-0.2]
---
## Problem

Only `prec` is exposed on Context. CPython Context has rounding, traps, Emin/Emax,
and `localcontext` for scoped overrides.

## Acceptance

- Minimum: `rounding` attribute accepted (even if only ROUND_HALF_EVEN implemented).
- `localcontext()` as context manager saving/restoring prec (and rounding if present).
- Traps: either implement InvalidOperation/DivisionByZero flags or CONFORMANCE permanent subset.
- Depends on per-interpreter prec fix.

## Paths

`src/eval/modules/decimal_mod.rs`, contextlib interaction, tests.
