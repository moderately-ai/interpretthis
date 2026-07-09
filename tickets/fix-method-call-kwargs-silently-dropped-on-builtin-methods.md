---
id: fix-method-call-kwargs-silently-dropped-on-builtin-methods
title: "Fix: method-call kwargs silently dropped on builtin methods"
status: done
priority: p0
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [eval/functions]
shared_scopes: []
paths: []
tags: [bug, inventory, parity]
---
## Bug
Builtin method calls (s.split(maxsplit=2), list.sort(reverse=True), …) drop kwargs; only positionals reach method dispatchers. Free functions and module funcs get kwargs.
## Ref
CONFORMANCE.md#method-call-kwargs
## User impact
Silent wrong behavior — high severity.
