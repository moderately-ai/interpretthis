---
id: gap-complex-default-argument-source-roundtrip
title: "Gap: complex default-argument source round-trip in state persistence"
status: ready
priority: p3
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/functions/definitions.rs, crates/interpretthis/src/serialize.rs, crates/interpretthis/tests/integration/state_persistence.rs]
tags: [gap, state, functions]
---
Audit source fallback: default argument ASTs are unparsed by a limited custom unparser; complex expressions can become `None # unparseable`, which will not faithfully reparse after state import if default_values are absent or future compatibility paths need source. Store/evaluate defaults robustly for complex expressions or remove the source fallback dependency.
