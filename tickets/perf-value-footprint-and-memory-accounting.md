---
id: perf-value-footprint-and-memory-accounting
title: "Performance: shrink Value footprint or align memory accounting"
status: ready
priority: p1
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [src/value.rs, src/state.rs, src/serialize.rs, tests/integration/resource_limits.rs, benches/memory.rs]
tags: [perf, audit, memory]
---
Audit found `std::mem::size_of::<Value>() == 136` while memory accounting uses `VALUE_SLOT_BYTES = 64`. Decide whether to (a) box large variants to reduce every container slot, (b) update accounting to match reality and refresh memory expectations, or (c) document the approximation. This is high-impact for list/dict-heavy memory and cache behavior but requires careful state serialization/resource-limit validation.
