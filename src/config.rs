// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

/// Configuration for the interpreter's resource limits and concurrency.
///
/// `Default::default()` provides sensible production defaults; callers
/// override individual fields with struct update syntax.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InterpreterConfig {
    /// Maximum AST operations before execution is aborted. Default: 10M.
    pub max_operations: u64,
    /// Maximum iterations per while loop. Default: 100K.
    pub max_while_iterations: u64,
    /// Maximum interpreter state size in bytes. Default: 128MB.
    pub max_memory_bytes: u64,
    /// Maximum captured print output in bytes. Default: 64KB.
    pub max_stdout_bytes: u64,
    /// Maximum concurrent parallelizable tool executions. Default: 10.
    pub max_concurrent_tools: u32,
    /// Soft wall-clock budget. `None` means no limit (default).
    ///
    /// Checked cooperatively every 100 operations — a single long-running
    /// tool future is not pre-empted mid-await.
    pub max_execution_time: Option<std::time::Duration>,
    /// Maximum nested user-function / lambda call depth before the
    /// interpreter aborts with `RecursionLimitExceeded`. Default:
    /// 1000, matching CPython's `sys.getrecursionlimit()`. Without
    /// this, `def f(): f()` is only bounded indirectly by
    /// `max_memory_bytes` and surfaces as the less informative
    /// `LimitExceeded(memory)`.
    pub max_recursion_depth: u32,
    /// Maximum bit length of a Python int (including sign bit magnitude).
    /// Operations that would produce a larger int raise `OverflowError`.
    /// Default: 1_048_576 bits (~128 KiB of limbs) — enough for crypto-scale
    /// ints, not enough for trivial DoS via `2 ** (10**9)`.
    pub max_int_bits: u64,
}

impl Default for InterpreterConfig {
    fn default() -> Self {
        Self {
            max_operations: 10_000_000,
            max_while_iterations: 100_000,
            max_memory_bytes: 128 * 1024 * 1024,
            max_stdout_bytes: 64 * 1024,
            max_concurrent_tools: 10,
            max_execution_time: None,
            max_recursion_depth: 1000,
            max_int_bits: 1_048_576,
        }
    }
}
