---
id: split-conformance-parity-track-table-out-of-user-reference
title: Split CONFORMANCE parity track table out of user reference
status: done
priority: p2
dependencies: []
related: [epic-public-readiness-for-crates-io-github]
scopes: [docs]
shared_scopes: []
paths: []
tags: [public-readiness, docs]
---
## Problem
CONFORMANCE.md mixes user-facing divergence catalogue with long internal track-status bookkeeping (commit hashes, foundation numbers).

## Work
- Move track status table to STATUS.md or CONTRIBUTING appendix
- Keep CONFORMANCE as divergence + anchors only
- Update cross-links

## Done when
CONFORMANCE skimmable for crate users; track board still exists somewhere.
