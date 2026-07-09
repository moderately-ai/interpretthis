---
id: eager-generators-memory-blow-up-for-large-infinite-yield-streams
title: "Eager generators: memory blow-up for large/infinite yield streams"
status: closed
priority: p2
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [eval]
shared_scopes: []
paths: []
tags: [bug, inventory, generators]
closed_reason: wontdo
closed_note: Inherent to eager materialization model; resource limits (ops/memory) are the mitigation. True streaming needs full coroutine VM (out of scope for 0.1).
---
## Issue
Eager materialization can OOM or hit limits where CPython streams.
## Related
Generator protocol ticket; itertools infinite iterators comments.
