---
id: epic-post-0-2-hardening-and-parity
title: "Epic: post-0.2 hardening, correctness, and hygiene"
status: done
priority: p1
dependencies: []
related: [async-await-consumer-driven-reopen-criteria, bench-baselines-refresh-after-0-2, bigint-op-matrix-indices-shifts-methods, bigint-resource-limits-digits-bits-op-cost, cargo-deny-ci-advisories-and-licenses, classvalue-builder-default-boilerplate, conformance-and-status-truth-pass-for-0-2-0, copy-cycles-and-user-copy-hooks, decimal-context-rounding-traps-localcontext, decimal-prec-per-interpreter-not-process-global, dependabot-or-manual-dep-update-workflow-under-no-prs, descriptor-precedence-nondata-vs-instance-dict, eval-trampoline-shrink-pin-frame-stack, exception-group-nested-and-subgroup-split-api, exception-type-constructor-single-path, generator-true-suspend-frames-not-eager-buffer, list-matmul-config-flag-or-permanent-extension-doc, metaclass-prepare-init-and-ns-methods-fidelity, public-api-docs-bigint-state-exceptiongroup-behavior, python-int-helpers-centralize-policy, recursion-test-stack-policy-docs-and-helpers, sharedlist-sharedfields-lock-cost-profile, slots-true-layout-no-dict-and-inheritance, state-export-roundtrip-bigint-and-exception-groups, tool-timeout-parallel-and-exception-mapping-audit, typeobject-methods-slot-fn-pointers-no-cycle, value-enum-footprint-audit-and-box-heavy-variants]
scopes: [docs]
shared_scopes: []
paths: []
tags: [epic, post-0.2]
---
## Goal

Rollup for work discovered after the 0.2.0 language-surface expansion.
Covers correctness (multi-interpreter safety, BigInt edges, ExceptionGroup/metaclass depth),
performance (eval frame stack), security resource model, docs truth, and code hygiene.

## Out of scope

- Catalyzed monorepo rewire
- Full CPython stdlib
- Bytecode VM rewrite

## Success

Child tickets done or closed with rationale; CONFORMANCE/STATUS match shipped behavior.
