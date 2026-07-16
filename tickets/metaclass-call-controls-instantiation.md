---
id: metaclass-call-controls-instantiation
title: "A metaclass __call__ does not intercept instantiation"
status: open
priority: p3
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [eval]
shared_scopes: []
paths: [crates/interpretthis/src/eval/classes.rs]
tags: [parity, inventory, metaclass]
---
## Gap
When a class's metaclass defines `__call__`, instantiating the class should
dispatch through it, not the default `type.__call__`:

```python
class Meta(type):
    def __call__(cls, *a, **kw):
        print("meta call")
        return super().__call__(*a, **kw)
class M(metaclass=Meta): ...
M()   # CPython prints "meta call" ; ours instantiates directly
```

`instantiate` (classes.rs:1095) handles abstract/enum/exception/`__new__`/
`__init__` but never consults the metaclass's `__call__`.

## Why deferred
Faithful support needs (1) resolving `type(cls).__call__` and invoking it with
the class as the first argument, AND (2) routing `super().__call__(*a, **kw)`
*inside* that metaclass method back to the default instantiation (`__new__` +
`__init__`). The `super()` proxy in a metaclass method is the hard part — it
must resolve against the metaclass MRO (`Meta` → `type`) and land on the
built-in `type.__call__` behaviour, which our `Super`/`SuperClass` machinery
does not currently model for the metaclass-of-a-class case. A meaningful
metaclass-protocol extension, not a localized fix.
