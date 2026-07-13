---
id: gap-decimal-scientific-formatting
title: "Gap: Decimal str/repr scientific formatting for extreme magnitudes"
status: ready
priority: p3
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/value.rs, crates/interpretthis/tests/integration/parity_corpus/modules/decimal/**, CONFORMANCE.md]
tags: [gap, stdlib, decimal, repr]
---
Audit source comment: Decimal formatting uses BigDecimal::to_plain_string and matches common ranges, but CPython switches to scientific notation for very large/small magnitudes. Implement CPython-shape Decimal formatting thresholds or document a permanent divergence. Also add the referenced CONFORMANCE anchor if keeping the divergence.
