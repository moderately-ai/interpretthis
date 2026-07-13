// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Eval-dispatch layer benchmark module.
//!
//! Pure interpreter-dispatch cost on the hottest path: tight loops doing
//! integer/float arithmetic, branches, and small list appends. No stdlib
//! modules, no tool calls — every cycle reflects the evaluator's
//! `for`-loop + binary-op + assign + control-flow overhead.
//!
//! Most sensitive to the upcoming Track A dispatch refactor; if
//! `__add__`/`__eq__` migration regresses it more than 1.2× the per-PR
//! gate trips.

use std::time::Duration;

use criterion::{Criterion, criterion_group};

use crate::common::run_snippet;

const INT_LOOP: &str = r"
total = 0
hits = []
for i in range(2000):
    total = total + i * 3
    if i % 7 == 0:
        hits.append(i)
    elif i % 5 == 0:
        total = total - 1
result = (total, len(hits))
";

const FLOAT_LOOP: &str = r"
acc = 0.0
count = 0
for i in range(1500):
    x = i * 0.5 + 1.0
    acc = acc + x * x
    if x > 100.0:
        count = count + 1
mean = acc / 1500.0
";

const BRANCH_LOOP: &str = r"
buckets = [0, 0, 0, 0]
for i in range(1500):
    v = (i * 17) % 100
    if v < 25:
        buckets[0] = buckets[0] + 1
    elif v < 50:
        buckets[1] = buckets[1] + 1
    elif v < 75:
        buckets[2] = buckets[2] + 1
    else:
        buckets[3] = buckets[3] + 1
";

fn bench_eval(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio current-thread runtime");

    let mut group = c.benchmark_group("eval");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(50);

    group.bench_function("int_loop_2k", |b| b.iter(|| run_snippet(&runtime, INT_LOOP)));
    group.bench_function("float_loop_1500", |b| b.iter(|| run_snippet(&runtime, FLOAT_LOOP)));
    group.bench_function("branch_loop_1500", |b| b.iter(|| run_snippet(&runtime, BRANCH_LOOP)));

    group.finish();
}

criterion_group!(benches, bench_eval);
