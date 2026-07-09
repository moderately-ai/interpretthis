---
id: arbitrary-precision-int-beyond-i64
title: Arbitrary-precision int (beyond i64)
status: todo
priority: p2
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [core, eval]
shared_scopes: []
paths: []
tags: [divergence, inventory, core]
---
## Divergence
Value::Int is i64 with checked overflow; CPython int is arbitrary precision.
## Impact
Large integer algorithms fail or overflow where CPython succeeds.
