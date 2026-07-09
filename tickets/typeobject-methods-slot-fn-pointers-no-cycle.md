---
id: typeobject-methods-slot-fn-pointers-no-cycle
title: "TypeObject methods_slot: real fn-pointer tables without types↔eval cycle"
status: done
priority: p2
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [core, eval/functions]
shared_scopes: []
paths: []
tags: [refactor, post-0.2]
---
## Problem

`has_methods_table` is a bool marker; method dispatch remains a large match in
`method_dispatch`. Full `methods_slot` on TypeObject was blocked by types↔eval dependency cycle.

## Acceptance

- Per-type method dispatch is table-driven (fn pointers or phf map by type name).
- TypeObject either holds slots registered at init from lib.rs, or a parallel table keyed by type name lives next to TypeObject without cycle.
- dispatch_method becomes thin: lookup table → call.
- No behavior change; existing method kwargs tests stay green.

## Paths

`src/types.rs`, `src/eval/functions/method_dispatch.rs`, `src/lib.rs` init if needed.
