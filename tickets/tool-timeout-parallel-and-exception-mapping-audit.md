---
id: tool-timeout-parallel-and-exception-mapping-audit
title: "Tool timeout: parallel tools and Exception vs host error mapping"
status: done
priority: p2
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [tools, eval]
shared_scopes: []
paths: []
tags: [tools, correctness, post-0.2]
---
## Problem

Wall-clock remaining budget is applied to tools, and tool errors can be catchable as Exception.
Need audit of parallel tool batches, cancellation, and consistent mapping
(host ToolError vs in-script Exception).

## Acceptance

- Document matrix: sequential vs parallel timeout behavior.
- Parallel tools respect global remaining budget (no oversubscription).
- Catchable vs uncaught paths covered by tests.

## Paths

`src/tools/**`, tool_system tests.
