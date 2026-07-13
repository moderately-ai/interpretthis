---
id: gap-function-body-cache-name-collision
title: "Gap: function body cache key collisions for same-named nested functions"
status: ready
priority: p2
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/functions/definitions.rs, crates/interpretthis/src/eval/functions/dispatch.rs, crates/interpretthis/src/state.rs, crates/interpretthis/tests/integration/parity_corpus/descriptors/decorator_stack_order.py]
tags: [gap, functions, decorators, state]
---
Audit test comment: function_bodies is keyed by function name, so nested functions/wrappers with the same name can collide. Refactor cached function body keys to a stable unique id carried by FunctionDef/LambdaDef while preserving state import/export compatibility.
