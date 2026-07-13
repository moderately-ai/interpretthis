// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Containers layer benchmark module.
//!
//! List / dict / set / tuple ops + comprehensions at scale. Four shapes:
//!
//! - **list_comp** — `[x * x for x in range(10000)]`. Pure build cost.
//! - **dict_comp_filter** — `{x: x * x for x in range(10000) if x % 3 == 0}`. Comprehension with
//!   predicate; touches hash-keyed insert on every third iteration.
//! - **nested_comp** — flatten a 100×100 generator into a list. Catches regressions in
//!   inner-iterator handling.
//! - **dict_get_in_loop_10k** — 10k `d[5]` reads on a 100-entry dict. Isolates subscript cost from
//!   container build.

use std::time::Duration;

use criterion::{Criterion, criterion_group};

use crate::common::run_snippet;

const LIST_COMP_10K: &str = r"
squares = [x * x for x in range(10000)]
n = len(squares)
";

const DICT_COMP_FILTER_10K: &str = r"
m = {x: x * x for x in range(10000) if x % 3 == 0}
n = len(m)
";

const NESTED_COMP: &str = r"
grid = [[i * 100 + j for j in range(100)] for i in range(100)]
flat = [v for row in grid for v in row]
n = len(flat)
";

const DICT_GET_IN_LOOP: &str = r"
d = {i: i for i in range(100)}
total = 0
for _ in range(10000):
    total = total + d[5]
";

fn bench_containers(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio current-thread runtime");

    let mut group = c.benchmark_group("containers");
    group.measurement_time(Duration::from_secs(12));
    group.sample_size(30);

    group.bench_function("list_comp", |b| b.iter(|| run_snippet(&runtime, LIST_COMP_10K)));
    group.bench_function("dict_comp_filter", |b| {
        b.iter(|| run_snippet(&runtime, DICT_COMP_FILTER_10K));
    });
    group.bench_function("nested_comp_100x100", |b| b.iter(|| run_snippet(&runtime, NESTED_COMP)));
    group.bench_function("dict_get_in_loop_10k", |b| {
        b.iter(|| run_snippet(&runtime, DICT_GET_IN_LOOP));
    });

    group.finish();
}

criterion_group!(benches, bench_containers);
