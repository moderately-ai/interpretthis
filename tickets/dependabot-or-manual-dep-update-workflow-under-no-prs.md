---
id: dependabot-or-manual-dep-update-workflow-under-no-prs
title: Dependency update workflow while GitHub PRs stay disabled
status: done
priority: p3
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [meta]
shared_scopes: []
paths: []
tags: [ci, process, post-0.2]
---
## Problem

PRs are fully disabled; Dependabot is awkward. Dependencies still need periodic updates.

## Acceptance

- Documented process: manual `cargo update` + CI on main, or bot that opens branches
  for direct-push review, or scheduled workflow that fails on advisories.
- No requirement to re-enable PRs.
- CONTRIBUTING/RELEASING short section.

## Paths

`.github/`, RELEASING.md, CONTRIBUTING.md.
