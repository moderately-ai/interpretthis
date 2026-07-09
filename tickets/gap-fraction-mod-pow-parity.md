---
id: gap-fraction-mod-pow-parity
title: "Gap: Fraction modulo and power arithmetic parity"
status: ready
priority: p2
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [src/types.rs, src/eval/modules/fractions.rs, tests/integration/parity_corpus/modules/fractions/**, CONFORMANCE.md]
tags: [gap, stdlib, fractions, parity]
---
Audit source comment: Fraction Mod/Pow are intentionally left unsupported. Implement Fraction % and ** parity (including mixed numeric cases already covered for + - * / //) or document the exact divergence.
