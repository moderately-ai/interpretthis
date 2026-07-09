---
id: refactor-typeobject-promote-remaining-variants
title: "Refactor: promote remaining Value variants to explicit TypeObject slots"
status: ready
priority: p3
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [src/types.rs, src/eval/op.rs, src/eval/names.rs, tests/integration/**]
tags: [refactor, typeobject, gap-audit]
---
Audit source comment: OBJECT_TYPE still catches contains/slot behavior for variants not promoted to explicit TypeObject definitions (Function/Lambda/Instance/Exception/etc.). This is not a current user-visible parity bug, but the follow-up/refactor comment should be tracked. Promote remaining variants where useful or replace the catch-all comment with a permanent design rationale.
