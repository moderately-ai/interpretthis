---
id: optional-cargo-deny-dependency-license-policy
title: "Optional: cargo-deny dependency license policy"
status: closed
priority: p2
dependencies: []
related: [epic-public-readiness-for-crates-io-github]
scopes: [meta]
shared_scopes: []
paths: []
tags: [public-readiness, security, deps]
closed_reason: wontdo
closed_note: "Deferred for 0.1.0: dual-license docs and SPDX headers ship first; revisit cargo-deny after first crates.io publish if dependency policy is requested."
---
## Work
Decide whether to adopt cargo-deny for third-party license allowlists.
If yes: deny.toml + CI job; allow MIT/Apache-2.0/BSD/etc. matching dual-license stance.
If no: close with reason.

## Done when
Either deny check in CI or explicit "won't do for 0.1.0" note on ticket.
