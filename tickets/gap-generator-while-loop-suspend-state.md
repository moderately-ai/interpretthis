---
id: gap-generator-while-loop-suspend-state
title: "Gap: mid-if generator suspension for side-effect-before-yield in loops"
status: ready
priority: p4
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/functions/generators.rs, crates/interpretthis/src/state.rs]
tags: [gap, generators, parity]
---
Largely resolved: `while` loops with direct-statement yields, and `while` loops
whose bodies contain `if cond: yield` (the yield at the head of its branch, incl.
nested ifs and if/else) now suspend lazily — a filtered infinite stream like
`while True: (if n%2==0: yield n); n+=1` consumed via `islice` works, and
statements after the yield run exactly once.

Residual: a yield preceded by a *side-effecting* statement inside a conditional
branch of a loop (`while True: if cond: log(); yield`) still falls back to eager
buffering, because whole-`if` re-entry on resume would re-execute `log()`. For a
finite generator this is correct; for an infinite one it hits the loop-iteration
limit instead of suspending. A fully general fix needs true mid-statement
continuation state (freeze inside the `if` branch without re-running preceding
statements or the condition), i.e. a per-nesting resume stack that also covers
loops/try nested inside the while. Rare pattern; deferred.
