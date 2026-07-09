---
id: str-casefold-true-unicode-casefolding-ss-etc
title: str.casefold true Unicode casefolding (ß→ss etc.)
status: done
priority: p3
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [eval/functions]
shared_scopes: []
paths: []
tags: [divergence, inventory, parity]
---
## Divergence
casefold currently to_lowercase; not full Unicode casefold.
## Ref
src/eval/functions/methods/str.rs
