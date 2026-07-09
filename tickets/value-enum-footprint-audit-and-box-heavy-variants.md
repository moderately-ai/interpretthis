---
id: value-enum-footprint-audit-and-box-heavy-variants
title: Value enum footprint audit; box heavy variants if needed
status: todo
priority: p2
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [core]
shared_scopes: []
paths: []
tags: [performance, hygiene, post-0.2]
---
## Problem

Growing `Value` variants (BigInt, LruCache, datetime, etc.) increases every Value's size
and clone cost even for small ints/bools.

## Acceptance

- Measure `size_of::<Value>()` and largest variants; record in ticket comment or STATUS.
- Box any variant that dominates padding if it reduces size without hot-path pain.
- No functional change; nextest green; optional microbench note.

## Paths

`src/value.rs`, maybe serialize.
