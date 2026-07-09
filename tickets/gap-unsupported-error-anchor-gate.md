---
id: gap-unsupported-error-anchor-gate
title: "Gap: enforce CONFORMANCE anchors for unsupported-feature errors"
status: ready
priority: p2
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [src/**, CONFORMANCE.md, tests/integration/**]
tags: [gap, docs, hygiene]
---
Audit docs: user-visible unsupported/not-supported errors should include a CONFORMANCE anchor, but there is no automated gate and some generic catch-alls still do not provide specific anchors. Add a lint/test that scans error strings or targeted tests for async def/async for/async with/unsupported statements.
