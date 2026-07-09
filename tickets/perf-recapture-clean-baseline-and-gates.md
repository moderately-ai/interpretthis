---
id: perf-recapture-clean-baseline-and-gates
title: "Performance: recapture clean Criterion baseline and update gates"
status: ready
priority: p1
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [benches/**, benches/baseline.json, CONFORMANCE.md, STATUS.md]
tags: [perf, audit]
---
Clean-machine follow-up to the 2026-07-09 audit. Current long Criterion run was contaminated by high host load, so do not use it as a regression truth. Re-run `cargo bench --bench interpreter` on an idle machine, refresh `benches/baseline.json` if appropriate, and document which envelopes are binding. Include focused checks for dict assignment and generator drain so the two O(n^2) fixes stay protected.
