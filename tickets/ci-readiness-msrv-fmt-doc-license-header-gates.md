---
id: ci-readiness-msrv-fmt-doc-license-header-gates
title: "CI readiness: MSRV, fmt, doc, license-header gates"
status: done
priority: p1
dependencies: []
related: [epic-public-readiness-for-crates-io-github]
scopes: [meta]
shared_scopes: []
paths: []
tags: [public-readiness, ci]
---
## Work
Review `.github/workflows/ci.yml`:
- Keep license-headers job
- Ensure fmt/clippy/test/doc run on PRs
- Pin/document MSRV 1.85 consistently with Cargo.toml
- Optional: `cargo doc` with -D warnings already in docs job — verify
- Fail clearly if python3.12 missing for parity (or document skip)

## Done when
CI yaml matches standalone repo reality; no catalyzed monorepo assumptions.
