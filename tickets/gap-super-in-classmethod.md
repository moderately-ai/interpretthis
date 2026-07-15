---
id: gap-super-in-classmethod
title: "Gap: zero-arg super() inside a classmethod / __init_subclass__"
status: ready
priority: p3
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/classes.rs, crates/interpretthis/src/eval/functions/builtins.rs, crates/interpretthis/src/state.rs, crates/interpretthis/tests/integration/parity_corpus/misuse/class_advanced.py, CONFORMANCE.md]
tags: [gap, classes, super, parity]
---
Zero-arg `super()` only works inside a method whose receiver is a `Value::Instance`: `call_method` (classes.rs ~1263) pushes a `MethodFrame` only for instance receivers, and `super()` (builtins.rs ~433) then requires `live_self` to be an `Instance`. Inside a classmethod — including the common `def __init_subclass__(cls, **kw): super().__init_subclass__(**kw)` boilerplate — the receiver is a `Value::Class`, so `super()` raises `RuntimeError: super(): no current method frame`.

To fix: push a `MethodFrame` for class receivers too (store the class), have zero-arg `super()` build a class-bound Super proxy when the frame's receiver is a `Value::Class`, and resolve/dispatch the proxied method as a classmethod (cls = the runtime class) up the MRO — with `object.__init_subclass__` / `object.__init__` etc. resolving to no-ops. The `__init_subclass__` hook itself and the computed-class-attribute augmented assignment (`type(self)._count += 1`) already work; `misuse/class_advanced.py` currently omits the `super().__init_subclass__()` line pending this.
