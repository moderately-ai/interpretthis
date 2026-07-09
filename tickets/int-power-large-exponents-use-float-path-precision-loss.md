---
id: int-power-large-exponents-use-float-path-precision-loss
title: Int power large exponents use float path (precision loss)
status: todo
priority: p3
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [eval]
shared_scopes: []
paths: []
tags: [divergence, inventory, core]
---
## Divergence
r > 63 integer pow uses f64 path — precision/overflow differs from CPython bigint pow.
## Ref
src/eval/operations.rs pow path; THREAT_MODEL attack table notes
