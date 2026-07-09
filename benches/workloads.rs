// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Realistic cross-cutting application workloads benchmark module.
//!
//! Workload mined from realistic host-side `code_interpreter` integration
//! tests and the differential snippets under `tests/integration/`.
//! Hosts often register a `final_answer(...)` tool that these snippets
//! call; benches use an empty `Tools` registry on purpose so the numbers
//! reflect the interpreter loop cost, not tool-dispatch latency. Each
//! snippet drops the `final_answer(...)` call and replaces it with
//! `_result = (...)` so the value is built but not handed off — same
//! dispatch path on the build side.

use std::time::Duration;

use criterion::{Criterion, criterion_group};

use crate::common::run_snippet;

const EXTRACT_RECORDS: &str = r#"
rows = []
for i in range(500):
    rows.append({
        "id": i,
        "score": (i * 13) % 100,
        "category": "high" if (i * 13) % 100 >= 70 else ("mid" if (i * 13) % 100 >= 40 else "low"),
        "name": "row_" + str(i),
    })

high = [r for r in rows if r["category"] == "high"]
buckets = {"high": 0, "mid": 0, "low": 0}
total_score = 0
for r in rows:
    buckets[r["category"]] = buckets[r["category"]] + 1
    total_score = total_score + r["score"]

_result = {
    "count": len(high),
    "items": [r["name"] for r in high],
    "buckets": buckets,
    "avg_score": total_score / len(rows),
}
"#;

const FORMAT_LINES: &str = r#"
header = "id | name | score"
sep = "-" * len(header)
lines = [header, sep]
for i in range(400):
    name = "row_{:03d}".format(i)
    score = (i * 7 + 3) % 100
    lines.append(f"{i} | {name} | {score}")

summary = "rows: {n}, max_score: {m}".format(n=400, m=99)
report = "\n".join(lines) + "\n\n" + summary
_result = (len(report), summary)
"#;

const DICT_AGGREGATE: &str = r#"
events = []
for i in range(600):
    events.append({
        "user": "user_" + str(i % 25),
        "action": ["click", "view", "purchase"][i % 3],
        "amount": (i * 11) % 50 + 1,
    })

totals = {}
counts = {}
for e in events:
    k = e["user"]
    if k in totals:
        totals[k] = totals[k] + e["amount"]
        counts[k] = counts[k] + 1
    else:
        totals[k] = e["amount"]
        counts[k] = 1

averages = {k: totals[k] / counts[k] for k in totals}
sorted_users = sorted(totals.keys())
_result = {
    "totals": totals,
    "averages": averages,
    "users": sorted_users,
    "n_events": len(events),
}
"#;

fn bench_workloads(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio current-thread runtime");

    let mut group = c.benchmark_group("workloads");
    group.measurement_time(Duration::from_secs(12));
    group.sample_size(30);

    group.bench_function("extract_records_500", |b| {
        b.iter(|| run_snippet(&runtime, EXTRACT_RECORDS));
    });
    group.bench_function("format_lines_400", |b| b.iter(|| run_snippet(&runtime, FORMAT_LINES)));
    group
        .bench_function("dict_aggregate_600", |b| b.iter(|| run_snippet(&runtime, DICT_AGGREGATE)));

    group.finish();
}

criterion_group!(benches, bench_workloads);
