// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `bytes` method dispatch — `decode`, `hex`, `startswith`,
//! `endswith`, `split`, `replace`, `find`. CPython's full bytes API is
//! larger; we wire the commonly-used surface for agent-emitted code.

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    value::{Value, shared_list},
};

/// Dispatch a method call on a `bytes` receiver. CPython's full bytes
/// API is large; we wire the common ones used by agent-emitted code
/// (decode, hex, startswith/endswith, split, replace, find).
pub(crate) fn dispatch_bytes_method(
    b: &[u8],
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "decode" => {
            // CPython: bytes.decode(encoding="utf-8", errors="strict")
            let encoding = match args.first() {
                Some(Value::String(name)) => name.as_str(),
                None => "utf-8",
                _ => {
                    return Err(InterpreterError::TypeError(
                        "decode() argument must be str".into(),
                    )
                    .into());
                }
            };
            match encoding {
                "utf-8" | "utf_8" | "UTF-8" | "UTF_8" | "ascii" | "ASCII" => {
                    let s = std::str::from_utf8(b).map_err(|e| {
                        EvalError::from(InterpreterError::ValueError(format!("invalid utf-8: {e}")))
                    })?;
                    Ok(Value::String(s.into()))
                }
                other => {
                    Err(InterpreterError::ValueError(format!("unknown encoding: {other}")).into())
                }
            }
        }
        "hex" => {
            // CPython: bytes.hex() returns lowercase hex string.
            use std::fmt::Write as _;
            let mut out = String::with_capacity(b.len() * 2);
            for byte in b {
                let _ = write!(out, "{byte:02x}");
            }
            Ok(Value::String(out.into()))
        }
        "startswith" => {
            let Value::Bytes(prefix) = args.first().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "startswith() requires 1 argument".into(),
                ))
            })?
            else {
                return Err(InterpreterError::TypeError(
                    "startswith() argument must be bytes".into(),
                )
                .into());
            };
            Ok(Value::Bool(b.starts_with(prefix)))
        }
        "endswith" => {
            let Value::Bytes(suffix) = args.first().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "endswith() requires 1 argument".into(),
                ))
            })?
            else {
                return Err(InterpreterError::TypeError(
                    "endswith() argument must be bytes".into(),
                )
                .into());
            };
            Ok(Value::Bool(b.ends_with(suffix)))
        }
        "split" => {
            let sep = match args.first() {
                Some(Value::Bytes(s)) => s.as_slice(),
                Some(_) => {
                    return Err(InterpreterError::TypeError(
                        "split() argument must be bytes".into(),
                    )
                    .into());
                }
                None => &b" "[..],
            };
            if sep.is_empty() {
                return Err(InterpreterError::ValueError("empty separator".into()).into());
            }
            let mut parts = Vec::new();
            let mut start = 0usize;
            let mut i = 0usize;
            while i + sep.len() <= b.len() {
                if &b[i..i + sep.len()] == sep {
                    parts.push(Value::Bytes(b[start..i].to_vec()));
                    i += sep.len();
                    start = i;
                } else {
                    i += 1;
                }
            }
            parts.push(Value::Bytes(b[start..].to_vec()));
            Ok(Value::List(shared_list(parts)))
        }
        "replace" => {
            let Some(Value::Bytes(needle)) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "replace() first argument must be bytes".into(),
                )
                .into());
            };
            let Some(Value::Bytes(replacement)) = args.get(1) else {
                return Err(InterpreterError::TypeError(
                    "replace() second argument must be bytes".into(),
                )
                .into());
            };
            if needle.is_empty() {
                return Ok(Value::Bytes(b.to_vec()));
            }
            let mut out = Vec::with_capacity(b.len());
            let mut i = 0usize;
            while i + needle.len() <= b.len() {
                if &b[i..i + needle.len()] == needle.as_slice() {
                    out.extend_from_slice(replacement);
                    i += needle.len();
                } else {
                    out.push(b[i]);
                    i += 1;
                }
            }
            out.extend_from_slice(&b[i..]);
            Ok(Value::Bytes(out))
        }
        "find" => {
            let Some(Value::Bytes(needle)) = args.first() else {
                return Err(
                    InterpreterError::TypeError("find() argument must be bytes".into()).into()
                );
            };
            if needle.is_empty() {
                return Ok(Value::Int(0));
            }
            for i in 0..b.len().saturating_sub(needle.len() - 1) {
                if &b[i..i + needle.len()] == needle.as_slice() {
                    return Ok(Value::Int(i64::try_from(i).unwrap_or(-1)));
                }
            }
            Ok(Value::Int(-1))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'bytes' object has no attribute '{method}'"
        ))
        .into()),
    }
}
