---
id: classvalue-builder-default-boilerplate
title: ClassValue builder/default to stop field-miss boilerplate
status: todo
priority: p3
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [core, eval, eval/modules]
shared_scopes: []
paths: []
tags: [hygiene, post-0.2]
---
## Problem

Every synthetic class (decimal.Context, contextlib helpers, dynamic type()) hand-fills
ClassValue fields; new fields (slots, slot_names) cause multi-file churn.

## Acceptance

- `ClassValue::new(name)` or builder with safe defaults.
- All construction sites use it.
- Adding a field requires one default site.

## Paths

`src/value.rs`, classes.rs, modules that insert ClassValue.
