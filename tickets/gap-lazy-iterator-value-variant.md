---
id: gap-lazy-iterator-value-variant
title: "Gap: lazy Iterator Value variant instead of Vec materialization"
status: ready
priority: p2
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [src/types.rs, src/eval/op.rs, src/eval/functions/builtins.rs, src/value.rs, tests/integration/parity_corpus/iteration_protocol/**]
tags: [gap, iteration, performance]
---
Audit source comment: IterSlot materializes list/tuple/set/str/bytes/dict/range into Vec<Value>. Builtin iter()/next() now exist, but the underlying model is eager. Add a Value::Iterator-style stateful/lazy iterator representation for large or infinite-safe iterables where appropriate, preserving resource bounds.
