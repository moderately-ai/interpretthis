---
id: add-dependabot-for-actions-and-cargo
title: Add Dependabot for Actions and Cargo
status: closed
priority: p2
dependencies: []
related: [epic-post-0-1-hardening-and-product-backlog]
scopes: [meta]
shared_scopes: []
paths: []
tags: [post-release, deps, ci]
closed_reason: wontdo
closed_note: PRs fully disabled on the repo; Dependabot PRs cannot land. Revisit if contribution model changes.
---
## Why
No dependabot.yml; Actions still on Node 20 deprecation warnings; deps will rot.

## Work
- .github/dependabot.yml for github-actions (weekly) and cargo (weekly or monthly)
- Group minor/patch if useful
- Since PRs are disabled org-wide on this repo, either:
  (a) temporarily allow collaborator PRs for dependabot only, or
  (b) use dependabot with auto-merge disabled and re-enable collab PRs, or
  (c) document manual cargo update cadence instead

Decide (a/b/c) in ticket note before implementing.

## Done when
Policy chosen and either dependabot.yml lands or explicit "manual updates" note in RELEASING/CONTRIBUTING.
