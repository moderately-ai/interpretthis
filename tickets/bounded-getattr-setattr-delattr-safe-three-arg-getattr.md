---
id: bounded-getattr-setattr-delattr-safe-three-arg-getattr
title: Bounded getattr/setattr/delattr (safe three-arg getattr)
status: done
priority: p2
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [security, eval]
shared_scopes: []
paths: []
tags: [missing, inventory, security]
---
## Missing
getattr/setattr/delattr fully blocked. CONFORMANCE plans bounded forms (e.g. getattr(o, name, default)) without unbounded getattr(o, user_string) escape.
## Ref
CONFORMANCE.md#eval-exec
