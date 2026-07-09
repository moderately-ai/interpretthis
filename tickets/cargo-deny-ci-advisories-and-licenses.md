---
id: cargo-deny-ci-advisories-and-licenses
title: Add cargo-deny (advisories + licenses) to CI
status: done
priority: p2
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [meta, docs]
shared_scopes: []
paths: []
tags: [security, ci, post-0.2]
---
## Problem

cargo-deny was deferred for 0.1.0. Public crate should gate known advisories and license policy.

## Acceptance

- `deny.toml` with license allowlist matching dual MIT/Apache-2.0 + deps policy.
- CI job runs `cargo deny check`.
- Document how to update advisories DB in RELEASING or CONTRIBUTING.

## Paths

`.github/workflows/ci.yml`, `deny.toml`, docs.
