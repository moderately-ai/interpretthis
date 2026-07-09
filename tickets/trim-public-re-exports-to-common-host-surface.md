---
id: trim-public-re-exports-to-common-host-surface
title: Trim public re-exports to common host surface
status: done
priority: p3
dependencies: []
related: [epic-post-0-1-hardening-and-product-backlog]
scopes: [core]
shared_scopes: []
paths: []
tags: [post-release, api]
---
## Why
lib.rs re-exports many advanced types (ClassValue, FunctionDef, MatchValue, …).
Hosts typically need Interpreter, Config, Tools, Value, errors.

## Work
- Audit public API; keep essentials at crate root
- Move niche types to interpretthis::value only (semver-conscious — do before 1.0)
- rustdoc examples still compile

## Done when
Root re-export list documented in lib.rs; no broken doctests.
