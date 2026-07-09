---
id: exception-type-constructor-single-path
title: "ExceptionType construction: single path for direct and indirect calls"
status: done
priority: p1
dependencies: []
related: [epic-post-0-2-hardening-and-parity, exception-group-nested-and-subgroup-split-api]
scopes: [eval/functions, eval]
shared_scopes: []
paths: []
tags: [hygiene, correctness, post-0.2]
---
## Problem

Direct `ValueError("x")` used an inline constructor in `eval_call` while
`E = ValueError; E("x")` went through `call_value_as_function`. ExceptionGroup
only worked on one path until fixed. Dual paths will diverge again.

## Acceptance

- All ExceptionType constructions (including ExceptionGroup) share one helper.
- Direct name call and bound ExceptionType call are identical for all registered exception names.
- Unit/parity tests for ExceptionGroup + ValueError both styles.

## Paths

`src/eval/functions/call.rs`, `dispatch.rs`, maybe exceptions.rs helper.
