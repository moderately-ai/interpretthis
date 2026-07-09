---
id: exception-group-nested-and-subgroup-split-api
title: "ExceptionGroup: nested groups, subgroup/split, BaseExceptionGroup rules"
status: done
priority: p1
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [eval, eval/functions, core]
shared_scopes: []
paths: []
tags: [parity, exceptions, post-0.2]
---
## Problem

0.2.0 ships ExceptionGroup constructor + `except*` leaf split. Missing CPython surface:
nested groups, `.subgroup()` / `.split()`, BaseExceptionGroup vs ExceptionGroup matching
rules, non-group into `except*`, re-raise residual semantics edge cases.

## Acceptance

- Nested ExceptionGroup handled in except* (recursive match/split).
- `.exceptions`, `.subgroup(matcher)`, `.split(matcher)` behave as CPython 3.12 for the supported matcher forms (type / tuple of types minimum).
- BaseExceptionGroup is not a subclass of Exception for bare `except Exception` (document + test).
- Parity corpus pins nested + subgroup paths.

## Paths

`src/eval/exceptions.rs`, `src/value.rs` ExceptionValue, call/construct, names attributes.
