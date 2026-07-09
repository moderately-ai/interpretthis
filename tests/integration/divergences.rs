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

use std::collections::HashMap;

use interpretthis::{Interpreter, InterpreterConfig, InterpreterDeps, Tools};

fn interpreter() -> Interpreter {
    Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default())
}

/// CONFORMANCE.md#int-power-i64-overflow — superseded: BigInt promotion.
#[tokio::test]
async fn int_pow_promotes_beyond_i64() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
print(2 ** 10)
print(2 ** 100)
print(9223372036854775807 + 1)
"#,
            &Tools::new(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "{:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "1024\n1267650600228229401496703205376\n9223372036854775808");
}

/// interpretthis accepts 2-D list `@` as matrix multiply (numpy-like).
/// CPython raises TypeError for list @ list.
#[tokio::test]
async fn list_matmul_is_supported() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
a = [[1, 2], [3, 4]]
b = [[5, 6], [7, 8]]
print(a @ b)
"#,
            &Tools::new(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "{:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "[[19, 22], [43, 50]]");
}

/// Two interpreters must not share decimal prec (no process-global prec).
#[tokio::test]
async fn decimal_prec_is_per_interpreter() {
    let a = interpreter();
    let b = interpreter();
    let r1 = a
        .execute(
            "from decimal import Decimal, getcontext\ngetcontext().prec = 6\nprint(getcontext().prec)",
            &Tools::new(),
            HashMap::new(),
        )
        .await;
    assert!(r1.error.is_none(), "{:?}", r1.error);
    assert_eq!(r1.stdout.trim(), "6");

    let r2 = b
        .execute(
            "from decimal import getcontext\nprint(getcontext().prec)",
            &Tools::new(),
            HashMap::new(),
        )
        .await;
    assert!(r2.error.is_none(), "{:?}", r2.error);
    // Fresh interpreter keeps default 28.
    assert_eq!(r2.stdout.trim(), "28");
}

#[tokio::test]
async fn deepcopy_handles_list_cycle() {
    // Cycle correctness is covered by `copy_mod` unit tests (memo by Arc
    // identity). This integration check ensures deepcopy is independent of
    // the original for nested lists (no shared mutation).
    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
import copy
a = [1, [2, 3]]
b = copy.deepcopy(a)
b[1].append(4)
print(a[1])
print(b[1])
"#,
            &Tools::new(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "{:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "[2, 3]\n[2, 3, 4]");
}

#[tokio::test]
async fn max_int_bits_limits_power() {
    let mut cfg = InterpreterConfig::default();
    cfg.max_int_bits = 64;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);
    let resp = interp.execute("print(1 << 100)", &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected overflow for max_int_bits=64 on shift");
}
