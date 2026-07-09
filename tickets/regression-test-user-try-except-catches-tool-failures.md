---
id: regression-test-user-try-except-catches-tool-failures
title: "Regression test: user try/except catches tool failures"
status: ready
priority: p1
dependencies: []
related: [epic-post-0-1-hardening-and-product-backlog]
scopes: [tests, tools]
shared_scopes: []
paths: []
tags: [post-release, tests, tools]
---
## Why
Docs now correctly state tool errors become catchable Exception in user Python.
Only host-side propagation is tested (tool_system); no try/except regression.

## Work
- Integration test: tool returns ToolError; Python try/except Exception succeeds
- Assert host execute still fails if uncaught
- Keep scopes to tests + tools only

## Done when
cargo test covers both catch and uncaught paths; docs remain accurate.
