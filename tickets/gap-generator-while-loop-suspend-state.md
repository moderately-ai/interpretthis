---
id: gap-generator-while-loop-suspend-state
title: "Gap: true generator suspend frames for while-loop bodies"
status: ready
priority: p1
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/functions/generators.rs, crates/interpretthis/src/state.rs, crates/interpretthis/tests/integration/parity_corpus/generators/**, CONFORMANCE.md]
tags: [gap, generators, parity]
---
Audit source comment: while loops still fall back to eager Lazy buffers because true suspend state for while loops is not modelled. Extend GeneratorFrame with while-loop resume state so while-based generators suspend/resume without eager buffering.
