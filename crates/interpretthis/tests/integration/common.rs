// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shared helpers for the host-side integration test files (`engine_smoke.rs`,
//! `divergences.rs`, `security.rs`, `state_persistence.rs`, etc.).
//!
//! Differential parity testing now lives in `parity_corpus_runner.rs`, which
//! runs each `parity_corpus/**/*.py` snippet through both the interpretthis
//! interpreter and host `python3` and byte-compares stdout. These helpers
//! cover the cases the corpus runner can't reach: pinned-output assertions
//! for documented divergences and shape assertions for sandbox-side errors.

use std::collections::HashMap;

use interpretthis::{Interpreter, InterpreterConfig, InterpreterDeps, Tools};

/// Run `code` through a fresh interpreter, asserting it succeeds and prints
/// exactly `expected` (after trimming the trailing newline).
pub async fn assert_output(code: &str, expected: &str) {
    let resp = run(code).await;
    assert!(resp.error.is_none(), "code `{code}` errored: {:?}", resp.error);
    assert_eq!(resp.stdout.trim_end(), expected, "unexpected output for code `{code}`");
}

/// Run `code` through a fresh interpreter, asserting it raises an error.
pub async fn assert_error(code: &str) {
    let resp = run(code).await;
    assert!(
        resp.error.is_some(),
        "code `{code}` should have raised, but printed: {:?}",
        resp.stdout
    );
}

/// Execute `code` through a fresh interpreter with no tools or seed variables.
async fn run(code: &str) -> interpretthis::InterpreterResponse {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    interp.execute(code, &Tools::new(), HashMap::new()).await
}
