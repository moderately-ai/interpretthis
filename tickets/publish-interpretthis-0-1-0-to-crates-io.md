---
id: publish-interpretthis-0-1-0-to-crates-io
title: Publish interpretthis 0.1.0 to crates.io
status: done
priority: p0
dependencies: []
related: [epic-post-0-1-hardening-and-product-backlog]
scopes: [meta]
shared_scopes: []
paths: []
tags: [post-release, release]
---
## Why
Repo is public-ready and CI is green; crates.io still unpublished (docs.rs dead).

## Work
Follow RELEASING.md on clean main:
1. Confirm CHANGELOG 0.1.0 + Cargo.toml version
2. cargo package / cargo publish
3. Annotated tag v0.1.0 + GitHub Release notes from CHANGELOG
4. Verify docs.rs builds; drop any residual "after publish" wording if needed

## Needs
crates.io token (cargo login / CARGO_REGISTRY_TOKEN) — human step.

## Done when
crates.io/crates/interpretthis serves 0.1.0; docs.rs page exists; tag on origin.
