---
id: public-api-docs-bigint-state-exceptiongroup-behavior
title: "Public API/docs: host-visible BigInt, state format, ExceptionGroup behavior"
status: done
priority: p2
dependencies: []
related: [epic-post-0-2-hardening-and-parity, conformance-and-status-truth-pass-for-0-2-0]
scopes: [docs, core]
shared_scopes: []
paths: []
tags: [docs, post-0.2]
---
## Problem

Hosts need clear docs on what 0.2 changed for them: int overflow no longer always errors,
state blobs may grow, ExceptionGroup exists, async still unsupported.

## Acceptance

- Crate-level rustdoc / README notes for host-relevant behavior (not language fluff).
- Link CONFORMANCE for deep divergences.
- No marketing prose.

## Paths

`src/lib.rs`, `README.md`, CONFORMANCE cross-links.
