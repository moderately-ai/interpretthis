// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Consolidated criterion bench harness for interpretthis.
//!
//! One Cargo `[[bench]]` target; each layer/dimension lives in its own
//! sibling module. Same shape as `tests/integration/main.rs` — collapsing
//! seven separate bench binaries into one compile unit cuts bench-build
//! wall-clock proportionally on this dep graph.
//!
//! Modules:
//! - `eval` — eval-dispatch hot path (arithmetic / control flow)
//! - `frames` — function-call frame cost (recursion, closures, scope)
//! - `containers` — list / dict / set ops + comprehensions
//! - `dispatch` — builtin-type method dispatch
//! - `workloads` — cross-cutting realistic application snippets
//! - `memory` — peak memory_used_bytes accounting
//!
//! Each module exposes a `pub fn benches()` (via `criterion_group!`) that
//! this file collects into a single `criterion_main!` entry. Filter to one
//! group / one bench via criterion's CLI:
//!
//! ```text
//! cargo bench -- 'eval/'                       # whole eval group
//! cargo bench -- 'frames/recursive_fib_15'     # one bench
//! cargo bench -- 'parallel_throughput/8'       # criterion bench-id form
//! ```

#![expect(
    clippy::expect_used,
    reason = "benches are throwaway instrumentation; .expect() on runtime build \
              fails fast on bench-rig misconfiguration, which is what we want \
              from instrumentation"
)]

// Bench-time global allocator selection. The crate itself is
// allocator-neutral; the bench binary opts in via feature flag so
// recorded numbers reflect the production allocator (jemalloc, same
// as a typical production host). Mimalloc is available as an A/B alternative.
//
// At most one of the two features may be enabled at a time —
// `#[global_allocator]` is a static item, multiple declarations fail
// to link.
#[cfg(feature = "bench-alloc-jemalloc")]
#[global_allocator]
static GLOBAL_JEMALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(feature = "bench-alloc-mimalloc")]
#[global_allocator]
static GLOBAL_MIMALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod common;
mod containers;
mod dispatch;
mod eval;
mod frames;
mod memory;
mod workloads;

criterion::criterion_main!(
    eval::benches,
    frames::benches,
    containers::benches,
    dispatch::benches,
    workloads::benches,
    memory::benches,
);
