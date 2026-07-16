---
id: property-descriptor-object-via-class-access
title: "Accessing a property via the class returns the descriptor object"
status: open
priority: p3
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [eval, core]
shared_scopes: []
paths: [crates/interpretthis/src/eval/classes.rs]
tags: [parity, inventory, descriptors]
---
## Gap
`ClassName.prop` (a property accessed on the class, not an instance) should
return the `property` descriptor object, so `type(Temp.celsius).__name__ ==
'property'` and `Temp.celsius.fget/fset/fdel` are reachable. We currently raise
`AttributeError: type object 'Temp' has no attribute 'celsius'` because
`class_attribute` (classes.rs:1883) does not consult the `properties` map, and
there is no `property`-object `Value` representation.

Instance-level property access (`t.celsius`, the setter, read-only rejection)
all work — only the class-level descriptor object is missing.

## Why deferred
Needs a new `Value::Property` variant (or equivalent) threaded through
`type_name`, `Display`/`repr`, equality, serialization, and the descriptor
call/attribute paths — ~20+ match sites — for a niche introspection use whose
`Display` (`<property object at 0x…>`) is address-based and non-reproducible
anyway. Tracked rather than half-implemented (a `type_name`-only stub would
mislead by exposing a property object with none of its behaviour).

## Repro
```python
class Temp:
    @property
    def celsius(self): return 0
print(type(Temp.celsius).__name__)  # CPython: property ; ours: AttributeError
```
