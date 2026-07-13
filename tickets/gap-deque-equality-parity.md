---
id: gap-deque-equality-parity
title: "Gap: collections.deque equality parity"
status: ready
priority: p3
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/types.rs, crates/interpretthis/src/eval/functions/methods/deque.rs, crates/interpretthis/tests/integration/parity_corpus/modules/collections/**, CONFORMANCE.md]
tags: [gap, stdlib, collections, parity]
---
Audit source comment: deque equality returns NotImplemented/TypeError instead of CPython element-wise deque == deque. Add element-wise equality and inequality behaviour for deque.
