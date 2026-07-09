---
id: fix-stale-threat-model-claims-with-private-prefix-leftovers
title: Fix stale THREAT_MODEL claims (with + private-prefix leftovers)
status: done
priority: p0
dependencies: []
related: [epic-public-readiness-for-crates-io-github]
scopes: [docs]
shared_scopes: []
paths: []
tags: [public-readiness, docs, security]
---
## Problem
THREAT_MODEL.md still lists `with` as evaluator-rejected / Track F planned, and "Recently changed" / planned sections still treat `with` as pending. CONFORMANCE was updated; threat model drifted.

## Work
- Align evaluator-rejected table with `Stmt::With` → `eval_with`
- Remove or rewrite planned Track F bullets that claim with is unshipped
- Grep for other stale security claims after the docs pass
- Keep host-agnostic tone

## Done when
`rg -n 'Track F|no Stmt::With|private-prefix' THREAT_MODEL.md` only hits intentional history, not current false claims.
