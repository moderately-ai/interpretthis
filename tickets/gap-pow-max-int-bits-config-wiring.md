---
id: gap-pow-max-int-bits-config-wiring
title: "Gap: wire integer power through max_int_bits configuration"
status: ready
priority: p2
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/operations.rs, crates/interpretthis/src/config.rs, crates/interpretthis/tests/integration/resource_limits.rs, CONFORMANCE.md]
tags: [gap, resource-limits, numbers]
---
Audit source comment: integer power currently uses a fixed 1,048,576-bit cap in `pow_values` rather than fully honoring `InterpreterConfig::max_int_bits` through an arithmetic context. Wire pow through the configured limit consistently with shifts, preserving overflow/resource-limit tests.
