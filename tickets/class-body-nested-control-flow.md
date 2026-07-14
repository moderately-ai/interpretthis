---
id: class-body-nested-control-flow
title: Execute nested control flow in a class body
status: open
priority: p3
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [core, eval]
shared_scopes: []
paths: [crates/interpretthis/src/eval/classes.rs]
tags: [gap, inventory, classes]
---
## Gap
A class body only processes `FunctionDef`, `Assign`, and `AnnAssign` statements
directly; nested control flow is dropped by the `_ => {}` arm:

```python
class C:
    if COND:
        x = 1          # dropped -> C.x missing
    for i in range(3):
        total = ...    # dropped
```

Chained/tuple-unpacking assignments are now handled (see
`class_body_multi_assign.py`); this ticket is only the nested-control-flow case.

## Why it's separate
The correct fix executes the whole class body through the statement evaluator
against a scratch namespace (with enclosing-scope fallback for reads), then
harvests the resulting names into the class dict — the model CPython uses. That
is a real refactor of `eval_class_def`, which currently keeps method extraction
(FunctionDef -> methods/properties/staticmethods) interleaved with the class
dict. It risks regressing the enum / dataclass / slots / `__set_name__` /
metaclass paths, so it deserves focused work with those suites as guardrails.

## Acceptance
- `if`/`else`/`for`/`while`/nested-`class` statements in a class body assign
  into the class namespace; a class-body name reads enclosing-scope names.
- The enum/dataclass/slots/property/metaclass corpus stays green.
- A `parity_corpus/classes/` snippet with conditional and loop-built class
  attributes byte-diffs against python3.12.
