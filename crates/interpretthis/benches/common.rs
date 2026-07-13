// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shared helpers for the interpreter bench harness.
//!
//! Every layer/dimension bench creates a fresh `Interpreter`, executes a
//! snippet, and asserts the result was error-free. That's encapsulated
//! here so the per-module benches stay focused on the snippet + the
//! `bench_function` wiring.

use std::collections::HashMap;

use interpretthis::{Interpreter, InterpreterConfig, InterpreterDeps, Tools};

/// Build a fresh interpreter, run `code` to completion, and panic on
/// any execution error. The single-error-line panic surfaces the bench
/// label + the interpreter's error message together, which is what
/// criterion's per-bench failure output needs.
pub fn run_snippet(runtime: &tokio::runtime::Runtime, code: &str) {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let tools = Tools::new();
    let resp = runtime.block_on(interp.execute(code, &tools, HashMap::new()));
    assert!(resp.error.is_none(), "bench snippet errored: {:?}", resp.error);
}
