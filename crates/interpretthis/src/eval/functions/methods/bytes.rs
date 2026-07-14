// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `bytes` method dispatch — `decode`, `hex`, `startswith`,
//! `endswith`, `split`, `replace`, `find`. CPython's full bytes API is
//! larger; we wire the commonly-used surface for agent-emitted code.

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::functions::{opt_index_arg, to_index, to_len_i64},
    value::{ExceptionValue, Value, shared_list},
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
        "startswith" => bytes_affix(b, method, args, true),
        "endswith" => bytes_affix(b, method, args, false),
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
        "find" => bytes_search(b, method, args, false, false),
        "rfind" => bytes_search(b, method, args, true, false),
        "index" => bytes_search(b, method, args, false, true),
        "rindex" => bytes_search(b, method, args, true, true),
        "count" => {
            let (needle, bs, be) = bytes_search_args(b, method, args)?;
            Ok(Value::Int(to_len_i64(count_occurrences(&b[bs..be], &needle))?))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'bytes' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Coerce a `bytes` search needle: a `bytes` object, or an integer in
/// `range(0, 256)` treated as a single byte (CPython `b"abc".find(97)`).
fn bytes_needle(method: &str, arg: &Value) -> Result<Vec<u8>, EvalError> {
    match arg {
        Value::Bytes(v) => Ok(v.clone()),
        Value::Int(i) => u8::try_from(*i).map(|byte| vec![byte]).map_err(|_| {
            EvalError::from(InterpreterError::ValueError("byte must be in range(0, 256)".into()))
        }),
        Value::Bool(b) => Ok(vec![u8::from(*b)]),
        _ => Err(InterpreterError::TypeError(format!(
            "{method}() argument should be integer or bytes-like object, not '{}'",
            arg.type_name()
        ))
        .into()),
    }
}

/// Resolve `start`/`end` byte indices to a `[start, end)` byte window, matching
/// CPython's negative-index and clamp semantics.
fn byte_window(b: &[u8], start: Option<i64>, end: Option<i64>) -> (usize, usize) {
    let len = to_len_i64(b.len()).unwrap_or(i64::MAX);
    let clamp = |i: i64| -> i64 {
        let i = if i < 0 { i + len } else { i };
        i.clamp(0, len)
    };
    let start = clamp(start.unwrap_or(0));
    let end = clamp(end.unwrap_or(len)).max(start);
    (to_index(start).unwrap_or(0), to_index(end).unwrap_or(b.len()))
}

/// Shared argument parsing for the `bytes` search family: `(needle, byte_start,
/// byte_end)`.
fn bytes_search_args(
    b: &[u8],
    method: &str,
    args: &[Value],
) -> Result<(Vec<u8>, usize, usize), EvalError> {
    if args.is_empty() || args.len() > 3 {
        return Err(
            InterpreterError::TypeError(format!("{method}() takes at least 1 argument")).into()
        );
    }
    let needle = bytes_needle(method, &args[0])?;
    let (bs, be) = byte_window(b, opt_index_arg(args.get(1))?, opt_index_arg(args.get(2))?);
    Ok((needle, bs, be))
}

/// `find`/`rfind`/`index`/`rindex`: locate `needle` within the window and
/// return its absolute byte offset. `from_right` searches for the last match;
/// `raise_missing` raises `ValueError` instead of returning `-1`.
fn bytes_search(
    b: &[u8],
    method: &str,
    args: &[Value],
    from_right: bool,
    raise_missing: bool,
) -> EvalResult {
    let (needle, bs, be) = bytes_search_args(b, method, args)?;
    let hay = &b[bs..be];
    let found = if needle.is_empty() {
        Some(if from_right { be } else { bs })
    } else if from_right {
        hay.windows(needle.len()).rposition(|w| w == needle.as_slice()).map(|p| bs + p)
    } else {
        hay.windows(needle.len()).position(|w| w == needle.as_slice()).map(|p| bs + p)
    };
    match found {
        Some(pos) => Ok(Value::Int(to_len_i64(pos)?)),
        None if raise_missing => {
            Err(EvalError::Exception(ExceptionValue::new("ValueError", "subsection not found")))
        }
        None => Ok(Value::Int(-1)),
    }
}

/// Non-overlapping occurrence count (CPython `bytes.count`); an empty needle
/// matches at every gap, so `len + 1`.
fn count_occurrences(hay: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() {
        return hay.len() + 1;
    }
    let mut count = 0;
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if &hay[i..i + needle.len()] == needle {
            count += 1;
            i += needle.len();
        } else {
            i += 1;
        }
    }
    count
}

/// Shared `startswith`/`endswith` for `bytes`: honour the `start`/`end` window
/// and accept either a single `bytes` affix or a tuple of `bytes` affixes.
fn bytes_affix(b: &[u8], method: &str, args: &[Value], is_start: bool) -> EvalResult {
    if args.is_empty() || args.len() > 3 {
        return Err(
            InterpreterError::TypeError(format!("{method}() takes at least 1 argument")).into()
        );
    }
    let (bs, be) = byte_window(b, opt_index_arg(args.get(1))?, opt_index_arg(args.get(2))?);
    let window = &b[bs..be];
    let test = |affix: &[u8]| {
        if is_start { window.starts_with(affix) } else { window.ends_with(affix) }
    };
    let matched = match &args[0] {
        Value::Bytes(p) => test(p),
        Value::Tuple(items) => {
            let mut any = false;
            for it in items {
                let Value::Bytes(p) = it else {
                    return Err(InterpreterError::TypeError(format!(
                        "a bytes-like object is required for {method}()"
                    ))
                    .into());
                };
                if test(p) {
                    any = true;
                    break;
                }
            }
            any
        }
        _ => {
            return Err(InterpreterError::TypeError(format!(
                "{method}() first arg must be bytes or a tuple of bytes"
            ))
            .into());
        }
    };
    Ok(Value::Bool(matched))
}
