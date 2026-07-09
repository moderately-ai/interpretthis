---
id: async-await-consumer-driven-reopen-criteria
title: "async/await: document reopen criteria (no implementation)"
status: done
priority: p3
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [docs]
shared_scopes: []
paths: []
tags: [policy, async, post-0.2]
---
## Problem

async/await was closed wontdo for 0.x. Future work needs explicit consumer criteria
so it is not reopened casually.

## Acceptance

- CONFORMANCE or STATUS section: what would justify reopening (e.g. host needs
  await on tool futures without threads).
- No implementation in this ticket.
- Link prior closed ticket `async-await-and-coroutine-support`.

## Paths

`CONFORMANCE.md`, `STATUS.md`.
