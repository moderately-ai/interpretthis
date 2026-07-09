---
id: sharedlist-sharedfields-lock-cost-profile
title: Profile SharedList/SharedFields lock cost; document single-thread model
status: todo
priority: p3
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [core, benches]
shared_scopes: []
paths: []
tags: [performance, post-0.2]
---
## Problem

Identity-correct Arc<Mutex<...>> pays lock overhead on every list method and field access.
If execute is always single-threaded per interpreter, RefCell or parking_lot strategies may help.

## Acceptance

- Benchmark append/getattr loops before/after any change.
- If no code change: document that Mutex is intentional for Send across await points.
- If change: preserve Send/Sync as required by async interpreter; tests for alias mutation.

## Paths

`src/value.rs`, benches, AGENTS or THREAT_MODEL note if model changes.
