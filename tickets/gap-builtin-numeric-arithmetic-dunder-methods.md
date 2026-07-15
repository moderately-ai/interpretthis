---
id: gap-builtin-numeric-arithmetic-dunder-methods
title: "Gap: builtin numeric arithmetic dunders not callable as bound methods"
status: ready
priority: p3
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/functions/method_dispatch.rs]
tags: [gap, builtins, dunder]
---
Binary arithmetic/bitwise dunders on builtin numeric types are not exposed as
callable methods: `(100).__divmod__(7)`, `(10).__add__(5)`, `(10).__pow__(3)`,
`(10).__mod__(3)`, `(2).__lshift__(3)`, etc. raise
`AttributeError: 'int' object has no attribute '__divmod__'`. CPython returns
the operation result (`(14, 2)`, `15`, `1000`, `1`, ...).

`try_builtin_dunder` already handles the unary/conversion dunders (`__abs__`,
`__index__`, `__float__`, `__round__`, `__floor__`, ...) and could route the
binary ones through `operations::apply_binop_builtin`. The blocker is that
`apply_binop_builtin` needs `decimal_prec` and `max_int_bits` (from
`InterpreterConfig`), which the stateless `try_builtin_dunder(obj, method, args)`
signature does not carry. Threading config (or sensible defaults) into that path
would unlock the whole family in one place. `__divmod__` additionally returns a
2-tuple `(a // b, a % b)` rather than a single op.

Low priority: explicitly calling arithmetic dunders is rare in real code (the
operators themselves work). Unbound-via-type dunders on builtins
(`bool.__int__(True)`) are an even narrower corner of the same area.
