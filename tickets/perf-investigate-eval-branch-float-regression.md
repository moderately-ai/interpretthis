---
id: perf-investigate-eval-branch-float-regression
title: "Performance: investigate eval branch/float loop regression"
status: done
priority: p1
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [src/eval/**, src/types.rs, benches/eval.rs]
tags: [perf, audit]
---
Historical baseline comparison suggests branch_loop_1500 and float_loop_1500 may have regressed after recent eval/generator changes, but the latest Criterion run was load-contaminated. On a clean host, profile these loops specifically. Likely suspects: per-arm boxed futures in eval, extra dispatch indirection, memory-accounting checks, or rich-op/type-slot paths. Preserve recursion-safety gains unless a replacement trampoline/stack strategy exists.
