---
id: gap-instance-dict-key-equality-dunder-parity
title: "Gap: dict/set key equality for user __eq__/__hash__ parity"
status: ready
priority: p2
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/value.rs, crates/interpretthis/src/eval/operations.rs, crates/interpretthis/src/types.rs, crates/interpretthis/tests/integration/parity_corpus/classes/**]
tags: [gap, classes, dicts, parity]
---
Audit source comment: ValueKey::Instance equality falls back to structural field comparison because async __eq__ cannot run in hash-key equality. Classes whose __eq__ differs from structural equality drift from CPython in dict/set semantics. Design a safe model for user __eq__/__hash__ in mapping/set keys or document a permanent divergence.
