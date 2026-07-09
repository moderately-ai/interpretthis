---
id: conformance-and-status-truth-pass-for-0-2-0
title: CONFORMANCE.md and STATUS.md truth pass for 0.2.0 shipped surface
status: ready
priority: p0
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [docs]
shared_scopes: []
paths: []
tags: [docs, post-0.2]
---
## Problem

Several CONFORMANCE/STATUS sections still claim "out of scope" or "not yet" for features
shipped in 0.2.0 (ExceptionGroup, bigint, from_float, descriptors, getcontext, etc.).
Stale docs are a false contract for agents and hosts.

## Acceptance

- Every shipped 0.2.0 feature has an accurate CONFORMANCE status (Shipped / intentional divergence).
- STATUS.md tracks match reality (A1–A6, modules, security).
- Remove or rewrite legacy notes that contradict current code (e.g. old int-power OverflowError wording).
- No new fluff; only correct non-obvious behavior.

## Paths

`CONFORMANCE.md`, `STATUS.md`, cross-links from module docs if needed.
