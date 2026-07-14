---
id: model-frozenset-distinctly
title: Model frozenset as a distinct immutable, hashable type
status: open
priority: p3
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [core, eval, bindings]
shared_scopes: []
paths: [crates/interpretthis/src/value.rs, crates/interpretthis-python/src/convert.rs, crates/interpretthis-node/src/convert.rs]
tags: [gap, inventory, types]
---
## Gap
The interpreter has no `frozenset`; only `Value::Set` (mutable, unhashable).
Consequences:
- The Python binding maps an inbound `frozenset` to `Value::Set`, so it
  round-trips as a *mutable* set and the sandbox can `.add()` to what was
  immutable (`convert.rs:212`).
- A `frozenset` used as a dict key / set element loses its hashability
  (`Value::Set` has no hash), so `{frozenset([1, 2]): "x"}` cannot cross the
  boundary or be built in the sandbox.
- The `frozenset(...)` builtin is not available inside the sandbox.

## Why it's separate
This is a core type addition (like `Value::Complex`), not a binding tweak: a new
`Value::Frozenset` (or a frozen flag on the set storage) with its own
TypeObject — eq/hash slots, no mutating methods, `frozenset()` builtin, set
algebra returning frozensets when appropriate — plus inbound/outbound binding
conversions on both the Python and Node sides. It threads through hashing,
dict/set keys (a `ValueKey::Frozenset`), and repr.

## Acceptance
- `frozenset([1, 2])` constructs an immutable, hashable value; `.add()` raises
  AttributeError; it works as a dict key / set member.
- Python binding round-trips `frozenset` as `frozenset` (not `set`); Node has no
  frozenset, so document the set projection.
- `parity_corpus/sets/` snippets byte-diff against python3.12 (construction,
  hashing as a key, set algebra, immutability error).
