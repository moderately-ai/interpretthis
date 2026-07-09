---
id: epic-public-readiness-for-crates-io-github
title: "Epic: public readiness for crates.io / GitHub"
status: done
priority: p1
dependencies: []
related: []
scopes: [docs, meta]
shared_scopes: []
paths: []
tags: [epic, public-readiness]
---
## Goal
Ship `interpretthis` as a credible standalone OSS crate: publishable to crates.io, browsable on GitHub, accurate docs, no monorepo residue.

## Done when
- [ ] crates.io `cargo publish --dry-run` clean
- [ ] GitHub remote + README badges/links accurate
- [ ] CHANGELOG present for 0.1.0
- [ ] Security/conformance docs match code (no stale Track F / private-prefix)
- [ ] Dependency license policy optional but decided
- [ ] Child tickets under tag `public-readiness` closed or explicitly deferred

## Non-goals
Language parity tracks (A1–G); catalyzed rewire; performance work.
