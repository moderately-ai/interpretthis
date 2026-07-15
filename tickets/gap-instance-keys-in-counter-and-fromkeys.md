---
id: gap-instance-keys-in-counter-and-fromkeys
title: "Gap: custom-hash instance keys in Counter() and dict.fromkeys()"
status: ready
priority: p3
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/modules/collections.rs, crates/interpretthis/src/eval/functions/]
tags: [gap, instance-keys, collections]
---
Instances with `__hash__`/`__eq__` now work as keys for dict literals,
`d[key]` read, `d[key] = v` write, `key in d`, `set` literals, and `set(...)`.
Two constructors still route instance keys through the sync `value_to_key`,
which reports them as `unhashable type: 'object'`:

- `collections.Counter([inst, inst, ...])` — tallying instance elements.
- `dict.fromkeys([inst, ...])` — building a dict keyed by instances.

Both need the async `dict_insert_instance_key_pub` / hash+`__eq__` path (as the
dict-literal and subscript-assignment sites now use) instead of `value_to_key`.
Lower priority — instance keys in these two constructors specifically are a
narrow corner; the common dict/set instance-key operations are covered.
