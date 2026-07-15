---
id: gap-lazy-iterator-value-variant
title: "Gap: lazy iteration for short-circuiting consumers over huge iterables"
status: ready
priority: p4
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/op.rs, crates/interpretthis/src/eval/comprehensions.rs]
tags: [gap, iteration, performance]
---
`op::iter` materialises list/tuple/set/str/bytes/dict/range and generator
*expressions* into a `Vec<Value>`. Range O(1) surfaces are all covered now
(membership, indexing, len, and `.index()`/`.count()`), and true generator
functions suspend lazily.

Residual: a short-circuiting or bounded consumer over a *very large* lazy
iterable materialises it first. `any(x > 500_000 for x in range(10_000_000))`
should stop at ~500k in CPython, but the generator expression eagerly buffers
its 10M-element range and hits the 10M-operations sandbox limit. Fixing it needs
a lazy iterator representation (a stateful `Value` yielding on demand) so
generator expressions and `any`/`all`/`next`/`in`/`islice` pull element-by-element
with early exit. Note the operations limit still caps non-short-circuiting huge
iterations regardless of laziness, so the observable win is bounded to
short-circuiting consumers. Perf/architecture item; deferred.
