// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The host-facing `Value` surface: the conversions a caller outside this crate
//! needs in order to build and read interpreter values. The language bindings
//! are the primary consumer, and they can only reach what is `pub`.

use interpretthis::{InterpreterError, Value, ValueKey, shared_list};

#[test]
fn to_key_folds_integral_float_onto_the_int_slot() {
    // The invariant that makes this worth exposing rather than reimplementing:
    // CPython's `hash(2.0) == hash(2)`, so `{2: x}[2.0]` must hit one slot. A
    // host that derived keys itself and missed this would build a dict holding
    // two equal-but-distinct keys, silently corrupting `in` / `len` / lookup.
    let from_float = Value::Float(2.0).to_key().expect("float is hashable");
    let from_int = Value::Int(2).to_key().expect("int is hashable");
    assert_eq!(from_float, from_int);
    assert!(matches!(from_float, ValueKey::Int(2)));

    // Non-integral floats keep their own slot.
    let fractional = Value::Float(2.5).to_key().expect("float is hashable");
    assert_ne!(fractional, from_int);
}

#[test]
fn to_key_rejects_unhashable_values() {
    let err =
        Value::List(shared_list(vec![Value::Int(1)])).to_key().expect_err("a list is not hashable");

    match err {
        InterpreterError::TypeError(msg) => {
            assert!(msg.contains("unhashable type"), "unexpected message: {msg}");
            assert!(msg.contains("list"), "message should name the type: {msg}");
        }
        other => panic!("expected TypeError, got {other:?}"),
    }
}

#[test]
fn to_key_round_trips_through_to_value() {
    for value in [
        Value::None,
        Value::Bool(true),
        Value::Int(-7),
        Value::String("id".into()),
        Value::Tuple(vec![Value::Int(1), Value::String("x".into())]),
    ] {
        let key = value.to_key().expect("hashable");
        assert_eq!(key.to_value(), value, "round trip failed for {value:?}");
    }
}
