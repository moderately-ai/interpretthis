---
id: add-operator-module
title: Add the `operator` stdlib module
status: open
priority: p2
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [core, eval, modules]
shared_scopes: []
paths: [crates/interpretthis/src/eval/modules/operator.rs, crates/interpretthis/src/eval/modules/mod.rs]
tags: [gap, inventory, modules, stdlib]
---
## Gap
`import operator` raises `ModuleNotFoundError`. The module is unimplemented.
Discovered while adding `itertools.accumulate(initial=)` coverage — the natural
test `accumulate(xs, operator.mul, initial=2)` could not run.

## Impact
`operator` is a common companion to `functools.reduce`, `itertools.accumulate`,
and `sorted`/`min`/`max` (`key=operator.itemgetter(1)`). LLM-generated Python
reaches for it constantly, so its absence blocks a whole idiom family.

## Desired surface
Register `OperatorModule` (one line in `modules/mod.rs` + a struct/impl in
`operator.rs`), covering:

- **Binary value-returning** (reuse `types::dispatch_binop` / `dispatch_lt` /
  `dispatch_eq` / `dispatch_contains`): `add sub mul truediv floordiv mod pow
  matmul and_ or_ xor lshift rshift lt le eq ne gt ge concat contains getitem
  is_ is_not`.
- **Unary value-returning**: `neg pos abs invert not_ truth index inv`.
- **Callable-returning** (the harder part — must yield a callable `Value`):
  `itemgetter(*items)`, `attrgetter(*names)`, `methodcaller(name, *a, **kw)`.
  These likely need a new callable variant (or a reuse of `Value::Partial`'s
  shape) so `call_value_as_function` can dispatch them. `itemgetter` is the
  highest-value member (sorted keys) and should not be skipped.

## Acceptance
- `parity_corpus/modules/operator_*.py` snippets byte-diff against python3.12,
  including `itemgetter`/`attrgetter` used as a `sorted(key=...)`.
- Unknown attribute → `AttributeError: module 'operator' has no attribute ...`.
- `is_`/`is_not` agree with the `is` operator's identity semantics.
