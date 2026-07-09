---
id: parity-user-class-dunder-slot-dispatch-a1-a6-spine
title: "Parity: user-class dunder slot dispatch (A1–A6 spine)"
status: todo
priority: p1
dependencies: []
related: [epic-post-0-1-hardening-and-product-backlog]
scopes: [eval, core]
shared_scopes: []
paths: []
tags: [parity, eval]
---
## Why
STATUS.md A1–A6 partial: builtins use TypeObject slots; hand-written user-class
__eq__/__lt__/__iter__/__getitem__/__getattr__ mostly do not.

## Work (epic-sized — split further when starting)
- Promote user-class dunders onto the same slot path as builtins
- Start with A1 __eq__/__hash__ (highest user surprise: pointer identity)
- Corpus pins for each slot family as it lands
- Update STATUS.md per slice

## Done when
At least A1 shipped with corpus; remaining A2–A6 tracked as child tickets or STATUS rows.
