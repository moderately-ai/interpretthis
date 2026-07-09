---
id: ordereddict-stop-exposing-move-to-end-on-plain-dict
title: "OrderedDict: stop exposing move_to_end on plain dict"
status: closed
priority: p3
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [eval/functions, eval/modules]
shared_scopes: []
paths: []
tags: [divergence, inventory, stdlib]
closed_reason: wontdo
closed_note: "Deliberate cheap divergence: OrderedDict is Dict; separate Value variant not worth it for one AttributeError. Documented in CONFORMANCE."
---
## Divergence
move_to_end works on plain dict; CPython AttributeError.
## Ref
CONFORMANCE.md#ordereddict-on-dict
