// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Method-dispatch layer benchmark module.
//!
//! Cost of dispatching builtin-type methods (string / list / dict /
//! tuple slots). Probes the `methods/*` tree so refactors that touch
//! method lookup, argument resolution, or the `dispatch_*_method`
//! routers are visible immediately.

use std::time::Duration;

use criterion::{Criterion, criterion_group};

use crate::common::run_snippet;

const STRING_JOIN: &str = r"
parts = []
for i in range(1000):
    parts.append(str(i))
result = ','.join(parts)
";

const STRING_METHOD_UPPER: &str = r#"
s = "a"
total = 0
for _ in range(10000):
    total = total + len(s.upper())
"#;

fn bench_dispatch(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio current-thread runtime");

    let mut group = c.benchmark_group("dispatch");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(50);

    group.bench_function("string_join_1000", |b| b.iter(|| run_snippet(&runtime, STRING_JOIN)));
    group.bench_function("string_method_upper_10k", |b| {
        b.iter(|| run_snippet(&runtime, STRING_METHOD_UPPER));
    });

    group.finish();
}

criterion_group!(benches, bench_dispatch);
