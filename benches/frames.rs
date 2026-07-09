// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Function-call frames layer benchmark module.
//!
//! Per-call frame cost: every user-function call allocates a
//! `state.variables.clone()` on entry and restores it on exit. This bench
//! measures that cost from four angles:
//!
//! 1. `recursive_fib_15` — tight body, minimal variable scope, ~2000 recursive frames. Probes
//!    per-call overhead at its purest.
//! 2. `class_heavy_200` — many module-level variables in scope when each call happens. Probes how
//!    the per-frame clone scales with scope width.
//! 3. `empty_function_call_10k` — 10k calls to a no-op function. Strips arithmetic/comparison out
//!    of the call-cost measurement that `recursive_fib_15` mixes.
//! 4. `closure_cell_read_10k` — closure cell read in a tight loop. Probes the closure-overlay /
//!    nonlocal cell-read path in isolation.

use std::time::Duration;

use criterion::{Criterion, criterion_group};

use crate::common::run_snippet;

const RECURSIVE_FIB: &str = r"
def fib(n):
    return 1 if n < 2 else fib(n - 1) + fib(n - 2)
result = fib(15)
";

const CLASS_HEAVY: &str = r"
class A:
    def __init__(self, x):
        self.x = x
    def hop(self, y):
        return self.x + y

class B:
    def __init__(self, a):
        self.a = a
    def step(self):
        return self.a.hop(2)

class C:
    def __init__(self):
        self.k = 1
    def shift(self, n):
        return n * self.k

class D:
    pass

class E:
    pass

a = A(5)
b = B(a)
c = C()
d = D()
e = E()
scratch1 = [1, 2, 3]
scratch2 = {'a': 1, 'b': 2}
scratch3 = (4, 5, 6)
scratch4 = 'hello'
scratch5 = 42

total = 0
for i in range(200):
    total = total + b.step()
    total = total + c.shift(i)
";

const EMPTY_FUNCTION_CALL: &str = r"
def f():
    pass
for _ in range(10000):
    f()
";

const CLOSURE_CELL_READ: &str = r"
x = 42
def outer():
    def inner():
        s = 0
        for _ in range(10000):
            s = s + x
        return s
    return inner()
result = outer()
";

fn bench_frames(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio current-thread runtime");

    let mut group = c.benchmark_group("frames");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(50);

    group.bench_function("recursive_fib_15", |b| b.iter(|| run_snippet(&runtime, RECURSIVE_FIB)));
    group.bench_function("class_heavy_200", |b| b.iter(|| run_snippet(&runtime, CLASS_HEAVY)));
    group.bench_function("empty_function_call_10k", |b| {
        b.iter(|| run_snippet(&runtime, EMPTY_FUNCTION_CALL));
    });
    group.bench_function("closure_cell_read_10k", |b| {
        b.iter(|| run_snippet(&runtime, CLOSURE_CELL_READ));
    });

    group.finish();
}

criterion_group!(benches, bench_frames);
