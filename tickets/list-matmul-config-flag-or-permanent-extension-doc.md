---
id: list-matmul-config-flag-or-permanent-extension-doc
title: "list @ matrix multiply: config flag or permanent-extension CONFORMANCE"
status: done
priority: p3
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [eval, docs, core]
shared_scopes: []
paths: []
tags: [policy, post-0.2]
---
## Problem

list@list is an intentional extension (CPython TypeErrors). Some hosts may want
strict CPython-identical mode.

## Acceptance

- Either: InterpreterConfig flag defaulting to current extension behavior, or
- Permanent CONFORMANCE extension with no flag (document decision).
- If flag: test both modes.

## Paths

`src/config.rs`, `src/eval/operations.rs`, CONFORMANCE, tests.
