---
id: a1-user-class-eq-hash-on-typeobject-slots
title: "A1: user-class __eq__/__hash__ on TypeObject slots"
status: in-progress
priority: p0
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [eval, core]
shared_scopes: []
paths: []
tags: [parity, inventory, a1]
claimed_from: ready
assignee: agent-main
lease_expires_at: 1783613355
---
## Gap
Builtins use TypeObject eq/hash slots; hand-written user-class __eq__ falls through to pointer identity. Dataclass-synthesized __eq__ works.
## Ref
STATUS.md A1; types.rs dispatch_eq
## Done when
User-class __eq__/__hash__ invoked via slots; corpus pins; STATUS A1 updated.
