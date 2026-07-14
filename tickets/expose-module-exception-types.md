---
id: expose-module-exception-types
title: Expose module exception types (statistics.StatisticsError, re.error, json.JSONDecodeError)
status: open
priority: p2
dependencies: []
related: [epic-full-gap-and-divergence-inventory]
scopes: [core, eval, modules]
shared_scopes: []
paths: [crates/interpretthis/src/eval/modules/mod.rs, crates/interpretthis/src/eval/modules/statistics.rs, crates/interpretthis/src/eval/modules/re.rs, crates/interpretthis/src/eval/modules/json.rs]
tags: [gap, inventory, modules, exceptions]
---
## Gap
Module-specific exception types cannot be named in `except` clauses:
`except statistics.StatisticsError:` / `except re.error:` /
`except json.JSONDecodeError:` all raise `AttributeError: module '…' has no
attribute '…'`. No module implements `constant()` for its error type.

Discovered while validating `statistics.median` — the natural test
`except statistics.StatisticsError` could not resolve the attribute.

## Related divergence
`typed_exception(...)` stores the type name module-qualified, e.g.
`"statistics.StatisticsError"`, so `type(e).__name__` would print
`statistics.StatisticsError` where CPython prints the bare `StatisticsError`.
Any fix must reconcile the stored name with `__name__`.

## Desired behaviour
- `module_member` resolves these names to a catchable exception-type value
  (`Value::ExceptionType`) via each module's `constant()`.
- `except <module>.<Error>:` matches an exception raised by that module.
- `type(e).__name__` returns the bare class name (`StatisticsError`,
  `error`/`PatternError`, `JSONDecodeError`) to match CPython.
- The `raise <module>.<Error>(msg)` construction form also works.

## Acceptance
- `parity_corpus/**` snippets for each module: raise-and-catch via the module
  attribute, plus `type(e).__name__`, byte-diffed against python3.12.
