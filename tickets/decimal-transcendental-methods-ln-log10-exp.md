---
id: decimal-transcendental-methods-ln-log10-exp
title: "Decimal.ln() / .log10() / .exp() are missing"
status: open
priority: p3
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [eval, modules]
shared_scopes: []
paths: [crates/interpretthis/src/eval/modules/decimal_mod.rs]
tags: [parity, inventory, decimal]
---
## Gap
`Decimal("10").ln()`, `.log10()`, and `.exp()` raise
`AttributeError: 'Decimal' object has no attribute 'ln'`. CPython returns a
result at the context precision (28 significant digits by default):

```python
Decimal(10).ln()  # 2.302585092994045684017991455
```

`.sqrt()` already works (bigdecimal provides it); the transcendentals do not.

## Why deferred
The backing `bigdecimal::BigDecimal` has no `ln`/`exp`/`log10`. Matching CPython
means arbitrary-precision transcendental evaluation (Taylor/Newton at the
context precision with correct rounding) — a real numerics task. An f64-based
approximation is rejected on purpose: it would return ~15 correct digits and
then wrong trailing digits, which is a worse divergence than the clean
AttributeError (both engines currently "error" on the parity harness, so no
stdout mismatch is produced). Implement only with a genuine arbitrary-precision
algorithm honouring `getcontext().prec`.
