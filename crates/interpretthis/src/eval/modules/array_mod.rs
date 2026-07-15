// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `array` module — `array.array(typecode, initializer)`,
//! a typed, homogeneous, mutable sequence. Stored as [`Value::Array`] (a shared
//! `Vec` plus the format typecode). Integer typecodes hold `Value::Int`, the
//! float typecodes (`'f'`/`'d'`) hold `Value::Float`; elements are coerced and
//! range-agnostic (we do not enforce the C width, only the int/float kind).

use crate::{
    error::{EvalResult, InterpreterError},
    eval::{control_flow::iterate_value, modules::need_arg},
    value::Value,
};

/// Whether `typecode` is a valid `array` format character.
fn is_valid_typecode(c: char) -> bool {
    matches!(c, 'b' | 'B' | 'h' | 'H' | 'i' | 'I' | 'l' | 'L' | 'q' | 'Q' | 'f' | 'd' | 'u')
}

/// Bytes per element for `typecode`, matching CPython on a 64-bit platform.
pub(crate) fn itemsize(typecode: char) -> usize {
    match typecode {
        'b' | 'B' => 1,
        'h' | 'H' => 2,
        'i' | 'I' | 'f' => 4,
        'u' => 4,
        _ => 8, // l L q Q d
    }
}

/// Coerce `value` to the element kind required by `typecode` (int vs float),
/// raising `TypeError` on a mismatch — CPython's array element checking.
pub(crate) fn coerce_element(typecode: char, value: &Value) -> EvalResult {
    let is_float = matches!(typecode, 'f' | 'd');
    if is_float {
        match value {
            Value::Float(_) => Ok(value.clone()),
            Value::Int(i) => Ok(Value::Float(*i as f64)),
            Value::Bool(b) => Ok(Value::Float(f64::from(*b))),
            _ => Err(InterpreterError::TypeError("array item must be a float".into()).into()),
        }
    } else if typecode == 'u' {
        match value {
            Value::String(_) => Ok(value.clone()),
            _ => {
                Err(InterpreterError::TypeError("array item must be a unicode character".into())
                    .into())
            }
        }
    } else {
        match value {
            Value::Int(_) | Value::BigInt(_) => Ok(value.clone()),
            Value::Bool(b) => Ok(Value::Int(i64::from(*b))),
            _ => Err(InterpreterError::TypeError("array item must be an integer".into()).into()),
        }
    }
}

pub struct ArrayModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for ArrayModule {
    fn name(&self) -> &'static str {
        "array"
    }
    fn constant(&self, name: &str) -> Option<Value> {
        // `array.typecodes` — the string of valid format characters.
        (name == "typecodes").then(|| Value::String("bBhHiIlLqQfdu".into()))
    }
    fn has_function(&self, name: &str) -> bool {
        // `array.array(...)` — the module exposes the class as a callable.
        name == "array" || name == "ArrayType"
    }
    async fn call(
        &self,
        _state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &indexmap::IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        match func {
            "array" | "ArrayType" => construct_array(args),
            _ => Err(InterpreterError::AttributeError(format!(
                "module 'array' has no attribute '{func}'"
            ))
            .into()),
        }
    }
}

/// `array.array(typecode[, initializer])`.
pub(crate) fn construct_array(args: &[Value]) -> EvalResult {
    let Value::String(tc) = need_arg("array", args, 0)? else {
        return Err(InterpreterError::TypeError(
            "array() argument 1 must be a unicode character, not ...".into(),
        )
        .into());
    };
    let mut chars = tc.chars();
    let (Some(typecode), None) = (chars.next(), chars.next()) else {
        return Err(InterpreterError::TypeError(
            "array() argument 1 must be a unicode character".into(),
        )
        .into());
    };
    if !is_valid_typecode(typecode) {
        return Err(InterpreterError::ValueError(
            "bad typecode (must be b, B, u, h, H, i, I, l, L, q, Q, f or d)".into(),
        )
        .into());
    }
    let mut items = Vec::new();
    if let Some(init) = args.get(1) {
        // bytes/bytearray initializers are the packed form — not modelled; the
        // common case is an iterable of numbers.
        for elem in iterate_value(init)? {
            items.push(coerce_element(typecode, &elem)?);
        }
    }
    Ok(Value::Array { typecode, items: crate::value::shared_list(items) })
}
