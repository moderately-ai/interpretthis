// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Single integration-test binary for the Python interpreter. Cargo discovers
//! this via the explicit `[[test]]` entry in `Cargo.toml`; per-concern
//! submodules sit alongside in this subdirectory. Consolidating the former
//! per-file targets into one binary shaves the parallel re-link of the crate's
//! full transitive closure off `cargo check --tests`.

mod common;
mod parity_corpus_runner;

mod conformance_anchors;
mod cpython_imported;
mod divergences;
mod engine_smoke;
mod host_value_api;
mod parallelization;
mod resource_limits;
mod security;
mod state_persistence;
mod tool_system;
