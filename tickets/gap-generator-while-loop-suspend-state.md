---
id: gap-generator-while-loop-suspend-state
title: "Gap: generator suspension for yields inside nested loops"
status: ready
priority: p4
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/functions/generators.rs]
tags: [gap, generators, parity]
---
Largely resolved. Generators now suspend correctly for yields inside `if`
statements at every level (top-level, and inside `while`/`for`/`try` bodies),
including yields *after* side-effecting statements in a branch — a dedicated
`if_stack` records the resume position so the branch is not re-run and the
condition is not re-evaluated. `while True: if cond: log(); yield` consumed via
`islice` suspends lazily with correct side-effect ordering.

Residual: a yield inside a `for`/`while`/`with`/`match` that is itself nested
inside an `if` branch (or a second `while` nested in a `while`) still forces the
eager fallback — those constructs can't resume mid-way through the current
steppers, and their whole-statement re-entry would restart earlier iterations.
For a finite generator this is correct; only an *infinite* generator with that
specific deep nesting hits the loop-iteration limit. Closing it needs the same
per-nesting resume-stack treatment extended to `for`/`while` when they appear
inside another suspendable construct. Rare pattern; deferred.
