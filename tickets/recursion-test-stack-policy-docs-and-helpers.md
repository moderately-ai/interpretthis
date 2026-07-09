---
id: recursion-test-stack-policy-docs-and-helpers
title: "Recursion tests: stack policy docs and helper for safe depths"
status: todo
priority: p2
dependencies: []
related: [epic-post-0-2-hardening-and-parity, eval-trampoline-shrink-pin-frame-stack]
scopes: [tests, docs]
shared_scopes: []
paths: []
tags: [hygiene, reliability, post-0.2]
---
## Problem

Recursion tests SIGABRT when max_recursion_depth exceeds native stack capacity.
Depths were hand-lowered; future frame growth will break them again silently.

## Acceptance

- AGENTS.md / test module docs state: set interpreter max below native overflow; prefer RecursionError over SIGABRT.
- Shared test helper for "unbounded recursion expects RecursionError" with a conservative depth.
- Optional: run deep recursion test on a large-stack thread until trampoline lands.

## Paths

`tests/integration/engine_smoke.rs`, `resource_limits.rs`, `AGENTS.md`.
