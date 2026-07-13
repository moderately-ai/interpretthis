---
id: gap-contextlib-contextmanager-decorator
title: "Gap: contextlib.contextmanager decorator support"
status: ready
priority: p2
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/modules/contextlib_mod.rs, crates/interpretthis/src/eval/functions/generators.rs, crates/interpretthis/tests/integration/parity_corpus/modules/contextlib/**, CONFORMANCE.md]
tags: [gap, stdlib, contextlib, generators]
---
Audit source comment: contextlib.contextmanager is rejected because it requires suspended generators. True for-based generator frames now exist, so implement @contextmanager for supported generator bodies or document a narrower rejection with tests.
