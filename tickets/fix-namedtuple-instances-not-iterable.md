---
id: fix-namedtuple-instances-not-iterable
title: "Fix: namedtuple instances not iterable"
status: ready
priority: p1
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [eval, eval/modules]
shared_scopes: []
paths: []
tags: [bug, inventory, parity]
---
## Bug
for x in nt / list(nt) TypeError; nt[i] and nt.field work.
## Ref
CONFORMANCE.md#namedtuple-iteration
## Depends
Likely A4 Instance iter_slot or namedtuple-specific iter.
