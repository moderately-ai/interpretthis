---
id: perf-reduce-function-call-frame-map-churn
title: "Performance: reduce function-call frame map churn"
status: done
priority: p1
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/functions/**, crates/interpretthis/src/state.rs, crates/interpretthis/src/value.rs, crates/interpretthis/benches/frames.rs]
tags: [perf, audit]
---
Focused release probe showed ~700 ns/call overhead for simple Python function calls. Audit call/return frame setup, touched-name capture, locals backup/restore, closure cells, and state variable map churn. Goal: improve hot function calls without regressing closure/nonlocal semantics or recursion safety. Add a targeted bench/probe before refactoring.
