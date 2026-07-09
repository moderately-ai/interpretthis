---
id: bench-baselines-refresh-after-0-2
title: Refresh criterion baselines after 0.2 (bigint, shared fields)
status: todo
priority: p3
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [benches, docs]
shared_scopes: []
paths: []
tags: [performance, post-0.2]
---
## Problem

0.2 changed int path, shared instance fields, and method dispatch. Stored baselines
may be stale for regression detection.

## Acceptance

- Re-run main bench suite; update `benches/baseline.json` if used.
- Note material regressions in CHANGELOG or STATUS if any.
- Document how to compare in RELEASING or benches README.

## Paths

`benches/**`.
