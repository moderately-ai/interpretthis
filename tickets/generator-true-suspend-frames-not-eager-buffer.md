---
id: generator-true-suspend-frames-not-eager-buffer
title: "Generators: true suspend frames (not eager Lazy buffer)"
status: done
priority: p1
dependencies: []
related: [epic-post-0-2-hardening-and-parity]
scopes: [eval, eval/functions, core]
shared_scopes: []
paths: []
tags: [parity, generators, post-0.2]
---
## Problem

Generator protocol methods exist on `Value::Lazy`, but bodies still eagerly materialize.
Large generators OOM; `GeneratorExit` / finally-on-close semantics are approximate.

## Acceptance

- Generator functions suspend at yield and resume with send/throw.
- close() injects GeneratorExit and runs finally.
- Memory stays O(frame) not O(all yields) for a streaming generator test.
- CONFORMANCE updated; eager path removed or only used as opt-in fallback.

## Paths

`src/eval/functions/generators.rs`, control_flow yield, state yield_stack/lazy_cursors.
