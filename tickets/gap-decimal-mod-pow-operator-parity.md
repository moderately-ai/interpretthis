---
id: gap-decimal-mod-pow-operator-parity
title: "Gap: Decimal modulo/power and operator-specific TypeError wording"
status: ready
priority: p2
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/types.rs, crates/interpretthis/src/eval/modules/decimal_mod.rs, crates/interpretthis/tests/integration/parity_corpus/modules/decimal/**, CONFORMANCE.md]
tags: [gap, stdlib, decimal, parity]
---
Audit comments/docs: Decimal Mod/Pow are not wired, and Decimal-vs-float TypeError wording is generic rather than operator-specific. Implement Decimal % / ** parity where feasible and improve TypeError messages to match CPython operator names, or document any permanent divergence.
