---
id: gap-builtin-numeric-arithmetic-dunder-methods
title: "Gap: unbound builtin dunder access via the type object"
status: ready
priority: p4
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/functions/dispatch.rs]
tags: [gap, builtins, dunder]
---
RESOLVED for the common case: bound binary arithmetic/bitwise dunders on
builtins now work — `(10).__add__(5)`, `(100).__divmod__(7)`, `(10).__pow__(3)`,
`(10).__mod__(3)`, the reflected forms (`__radd__`, `__rsub__`, ...), and
`"a".__add__("b")` / `[1].__add__([2])` all evaluate via the shared binop.

Residual (very narrow): calling a dunder *unbound through the type object* with
an explicit self argument — `bool.__int__(True)`, `int.__add__(10, 5)`,
`str.__len__("x")`. Bound instance access works; only the `type.__dunder__(self,
...)` form is unhandled (it needs the type-object method surface to expose the
dunders as unbound callables). Extremely rare in real code — deprioritised.
