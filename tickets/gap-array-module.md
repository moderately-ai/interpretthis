---
id: gap-array-module
title: "Gap: array module (typed arrays) not implemented"
status: ready
priority: p3
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/modules/mod.rs, crates/interpretthis/src/value.rs]
tags: [gap, stdlib, array]
---
`import array` raises `ModuleNotFoundError`. The `array.array(typecode, initializer)`
type is a typed, homogeneous, mutable sequence with a fixed element type
(`'i'`, `'d'`, `'b'`, `'f'`, ...) and methods `append`, `extend`, `insert`,
`pop`, `remove`, `reverse`, `index`, `count`, `tolist`, `tobytes`, `frombytes`,
plus `.typecode`/`.itemsize` attributes and full sequence protocol
(indexing, slicing, iteration, len, `+`, `*`, comparison).

Unlike the other stdlib modules added so far, this needs a new container **value
type** (a `Value::Array { typecode, items }` variant or similar) with its own
render/eq/hash/getitem/setitem/iter wiring — not just a `Module` impl — because
the value must round-trip through the whole evaluator and both bindings. That
container work is the bulk of the effort; the module functions are thin once it
exists.

Lower priority: most extraction/reshaping code uses `list`; `array` matters
mainly for typed numeric buffers and `tobytes`/`frombytes` interop.
