---
id: wall-clock-timeout-does-not-pre-empt-blocked-tool-futures
title: Wall-clock timeout does not pre-empt blocked tool futures
status: todo
priority: p2
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [core, tools]
shared_scopes: []
paths: []
tags: [bug, inventory, tools]
---
## Limitation
max_execution_time checked every 100 ops; cannot cancel in-flight tool await.
## Work
Document remains; optional cancellation/timeouts on tool futures.
## Ref
src/config.rs; THREAT_MODEL limits
