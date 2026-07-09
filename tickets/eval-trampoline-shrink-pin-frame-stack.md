---
id: eval-trampoline-shrink-pin-frame-stack
title: "Eval trampoline: shrink Box::pin per-frame native stack"
status: todo
priority: p1
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [eval, eval/functions, core]
shared_scopes: []
paths: []
tags: [performance, reliability, post-0.2]
---
## Problem

Each Python call frame costs large native stack because `eval_stmt`/`eval_expr`
`Box::pin` futures hold full match-arm state. Recursion smoke tests keep lowering
depth; realistic `max_recursion_depth` is unusable on default OS stacks.

## Acceptance

- Measurable reduction in native stack per recursive Python call (document method).
- Default-thread tests support at least depth ~20–50 without SIGABRT (target TBD in ticket comments after profiling).
- Interpreter RecursionError still fires at configured max before host abort for unbounded recursion tests.
- No behavioral regressions in nextest suite.

## Approach options (pick one in implementation)

- Match dispatch outside Pin; thin async leaves
- Trampoline / explicit stack machine for calls
- Stacker or large-stack worker only as interim (not preferred long-term)

## Paths

`src/eval/mod.rs`, call/function paths, engine_smoke/resource_limits tests.
