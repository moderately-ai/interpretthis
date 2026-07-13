---
id: gap-class-keyword-arguments-init-subclass
title: "Gap: class definition keyword arguments beyond metaclass"
status: ready
priority: p2
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/classes.rs, crates/interpretthis/tests/integration/parity_corpus/classes/**, CONFORMANCE.md]
tags: [gap, classes, parity]
---
Audit source error: class keyword arguments other than metaclass are rejected. CPython forwards arbitrary class keywords to metaclass / __init_subclass__. Implement safe forwarding for supported hooks or document rejected keyword behaviour with tests.
