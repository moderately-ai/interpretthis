// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Memory-accounting dimension benchmark module.
//!
//! Three workloads that snapshot `Interpreter::memory_used_bytes()` —
//! the interpreter's own internal accounting counter — after a
//! deterministic load. Catches accounting drift: a refactor that
//! double-counts each value, or that forgets to release allocation on
//! overwrite, moves these numbers materially.
//!
//! Criterion's `Throughput::Bytes` tag carries the byte count as the
//! reported throughput; the JSON output history then has both the
//! wall-clock and the bytes-used number per run.

use std::{collections::HashMap, time::Duration};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group};
use interpretthis::{Interpreter, InterpreterConfig, InterpreterDeps, Tools};

const LIST_10K_STRINGS: &str = r"
xs = [str(i) for i in range(10000)]
n = len(xs)
";

const DICT_10K_ENTRIES: &str = r"
d = {i: i * i for i in range(10000)}
n = len(d)
";

const SEQUENTIAL_GROWTH_APPEND: &str = r"
xs.append('row_' + str(len(xs)))
";

const SEQUENTIAL_GROWTH_SEED: &str = r"
xs = []
";

fn run_and_snapshot(runtime: &tokio::runtime::Runtime, code: &str) -> usize {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let tools = Tools::new();
    let resp = runtime.block_on(interp.execute(code, &tools, HashMap::new()));
    assert!(resp.error.is_none(), "memory bench errored: {:?}", resp.error);
    interp.accounted_bytes()
}

fn run_sequential_growth_and_snapshot(runtime: &tokio::runtime::Runtime, count: usize) -> usize {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let tools = Tools::new();
    runtime.block_on(async {
        let resp = interp.execute(SEQUENTIAL_GROWTH_SEED, &tools, HashMap::new()).await;
        assert!(resp.error.is_none(), "seed errored: {:?}", resp.error);
        for _ in 0..count {
            let resp = interp.execute(SEQUENTIAL_GROWTH_APPEND, &tools, HashMap::new()).await;
            assert!(resp.error.is_none(), "append errored: {:?}", resp.error);
        }
    });
    interp.accounted_bytes()
}

fn bench_memory(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio current-thread runtime");

    let mut group = c.benchmark_group("memory");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(20);

    let baseline_list = run_and_snapshot(&runtime, LIST_10K_STRINGS);
    group.throughput(Throughput::Bytes(baseline_list as u64));
    group.bench_with_input(
        BenchmarkId::new("list_10k_strings", baseline_list),
        &LIST_10K_STRINGS,
        |b, code| {
            b.iter(|| {
                let used = run_and_snapshot(&runtime, code);
                assert_eq!(used, baseline_list, "list_10k_strings accounting drifted");
            });
        },
    );

    let baseline_dict = run_and_snapshot(&runtime, DICT_10K_ENTRIES);
    group.throughput(Throughput::Bytes(baseline_dict as u64));
    group.bench_with_input(
        BenchmarkId::new("dict_10k_entries", baseline_dict),
        &DICT_10K_ENTRIES,
        |b, code| {
            b.iter(|| {
                let used = run_and_snapshot(&runtime, code);
                assert_eq!(used, baseline_dict, "dict_10k_entries accounting drifted");
            });
        },
    );

    let baseline_growth = run_sequential_growth_and_snapshot(&runtime, 100);
    group.throughput(Throughput::Bytes(baseline_growth as u64));
    group.bench_with_input(
        BenchmarkId::new("sequential_growth_100x", baseline_growth),
        &100_usize,
        |b, &count| {
            b.iter(|| {
                let used = run_sequential_growth_and_snapshot(&runtime, count);
                assert_eq!(used, baseline_growth, "sequential_growth_100x accounting drifted");
            });
        },
    );

    group.finish();
}

criterion_group!(benches, bench_memory);
