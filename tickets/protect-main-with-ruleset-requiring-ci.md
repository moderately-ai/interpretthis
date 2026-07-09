---
id: protect-main-with-ruleset-requiring-ci
title: Protect main with ruleset requiring CI
status: ready
priority: p1
dependencies: []
related: [epic-post-0-1-hardening-and-product-backlog]
scopes: [meta]
shared_scopes: []
paths: []
tags: [post-release, security, ci]
---
## Why
main has no branch protection/rulesets. Anyone with write can force-push or skip CI.

## Work
- Create a repo ruleset (or classic branch protection) on main:
  - Require status checks: test (ubuntu), test (macos), docs, license headers, package
  - Restrict force-push / deletion
  - Optionally require linear history
- Document in RELEASING.md that release tags come from protected main
- Note: PRs are fully disabled — ruleset still protects direct pushes

## Done when
gh api shows ruleset or protection on main; unauthorized force-push denied.
