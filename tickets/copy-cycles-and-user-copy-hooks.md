---
id: copy-cycles-and-user-copy-hooks
title: "copy: cyclic structures and __copy__/__deepcopy__ hooks"
status: done
priority: p2
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [eval/modules, core]
shared_scopes: []
paths: []
tags: [parity, copy, post-0.2]
---
## Problem

Shallow/deep distinction works for acyclic lists/instances. Cycles can stack-overflow
or infinite-loop; user `__copy__` / `__deepcopy__` / `__reduce__` not honored.

## Acceptance

- deepcopy of a cyclic list/dict/instance terminates and preserves cycle shape.
- If `__deepcopy__` / `__copy__` defined on user class, call it (with memo for deep).
- Document unsupported pickle reduce paths.
- Tests for cycle + hook.

## Paths

`src/eval/modules/copy_mod.rs`, value traversal, tests.
