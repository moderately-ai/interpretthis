// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pinned divergences from CPython.
//!
//! Each test here exercises a behaviour where interpretthis *intentionally*
//! prints something different from CPython. A differential parity-corpus
//! snippet cannot live here — the corpus byte-compares against `python3` —
//! so the expected interpretthis output is transcribed inline and the leading
//! comment links to `CONFORMANCE.md` for the rationale.
//!
//! Adding to this file is a CONFORMANCE.md change first, code change second.
//! New divergences without a CONFORMANCE.md anchor are review-rejected.
//!
//! Status: the previous `collections.Counter` repr divergence was closed
//! in Track B3, which promoted Counter to a first-class `Value::Counter`
//! variant with the CPython-matching `Counter({...})` repr. The test
//! moved to the parity corpus under
//! `parity_corpus/modules/collections/counter_repr.py`.
