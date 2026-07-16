---
id: eq-dunder-notimplemented-for-unrelated-types
title: "int.__eq__(str) etc. should return NotImplemented, not False"
status: open
priority: p3
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [eval]
shared_scopes: []
paths: [crates/interpretthis/src/eval/functions/method_dispatch.rs]
tags: [parity, inventory, dunders]
---
## Gap
An explicit rich-equality dunder call between unrelated builtin types returns a
bool where CPython returns the `NotImplemented` singleton:

```python
(5).__eq__("x")   # CPython: NotImplemented ; ours: False
"a".__eq__(5)     # CPython: NotImplemented ; ours: False
[1].__eq__((1,))  # CPython: NotImplemented ; ours: False
```

The ordering dunders (`__lt__`/`__le__`/`__gt__`/`__ge__`) already return
`NotImplemented` for incomparable operands (compare_lt errs → None →
`Value::NotImplemented`, method_dispatch.rs:1237-1240). `__eq__`/`__ne__` route
through `values_equal_pub`, which yields `false` for both "comparable but not
equal" and "incomparable", so the two cases can't be distinguished without a
per-type comparability table.

## Why deferred
This only manifests on an *explicit* `x.__eq__(y)` call — the `==` operator
already returns `False` correctly (it treats `NotImplemented` as "fall back",
ending in `False`). Faithful behaviour needs a per-broad-type comparability
predicate (numeric↔numeric, str↔str, list↔list, dict↔dict, set↔{set,frozenset},
None↔None, …) mirroring each builtin type's `__eq__` override. Mechanical but
broad, for a low-frequency introspection case.

## Fix sketch
In the `__eq__`/`__ne__` arm, compute `eq_comparable(obj, other)`; when false,
return `Value::NotImplemented` instead of `Bool(false)`. `values_equal_pub`
still decides equality when comparable.
