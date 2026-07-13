---
id: gap-pattern-matching-class-keyword-patterns
title: "Gap: class pattern keyword matching and __match_args__ parity"
status: ready
priority: p2
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/match_stmt.rs, crates/interpretthis/tests/integration/parity_corpus/**, CONFORMANCE.md]
tags: [gap, parity, pattern-matching]
---
Audit source comment: builtin/class pattern keyword patterns are not fully supported. Add CPython parity for class pattern keyword fields and __match_args__ handling where safe, or explicitly document/reject unsupported shapes with CONFORMANCE anchors and tests.
