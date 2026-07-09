---
id: gap-lru-cache-kwargs-memoization
title: "Gap: functools.lru_cache kwargs memoization"
status: ready
priority: p3
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [src/eval/functions/dispatch.rs, src/eval/modules/functools.rs, tests/integration/parity_corpus/modules/functools/**]
tags: [gap, stdlib, functools, parity]
---
Audit source comment: lru_cache memoizes by positional ValueKeys only; kwargs are unsupported for the cache key. Add deterministic keyword-argument keying matching CPython's lru_cache behaviour.
