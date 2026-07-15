// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `bytes` method dispatch — `decode`, `hex`, `startswith`,
//! `endswith`, `split`, `replace`, `find`. CPython's full bytes API is
//! larger; we wire the commonly-used surface for agent-emitted code.

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::functions::{MethodOutcome, opt_index_arg, to_index, to_len_i64},
    value::{ExceptionValue, SharedByteArray, Value, shared_bytes, shared_list},
};

/// Dispatch a method call on a `bytearray` receiver. Mutating methods act on
/// the shared storage in place; the read methods reuse `dispatch_bytes_method`
/// on a snapshot, converting any `bytes` result back to a `bytearray` (CPython's
/// bytearray methods return bytearray, and `split` returns a list of bytearray).
pub(crate) fn dispatch_bytearray_method(
    ba: &SharedByteArray,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let byte_arg = |v: &Value| -> Result<u8, EvalError> {
        match v {
            Value::Int(n) if (0..=255).contains(n) => Ok(*n as u8),
            Value::Bool(b) => Ok(u8::from(*b)),
            Value::Int(_) => {
                Err(InterpreterError::ValueError("byte must be in range(0, 256)".into()).into())
            }
            other => Err(InterpreterError::TypeError(format!(
                "'{}' object cannot be interpreted as an integer",
                other.type_name()
            ))
            .into()),
        }
    };
    let bytes_of = |v: &Value| -> Result<Vec<u8>, EvalError> {
        match v {
            Value::Bytes(b) => Ok(b.clone()),
            Value::ByteArray(b) => Ok(b.lock().clone()),
            other => Err(InterpreterError::TypeError(format!(
                "can't extend/concat bytearray with '{}'",
                other.type_name()
            ))
            .into()),
        }
    };
    match method {
        "append" => {
            let byte = byte_arg(args.first().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError("append() takes one argument".into()))
            })?)?;
            ba.lock().push(byte);
            Ok(MethodOutcome::grew(Value::None, 1))
        }
        "extend" => {
            let extra = bytes_of(args.first().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError("extend() takes one argument".into()))
            })?)?;
            let n = extra.len();
            ba.lock().extend_from_slice(&extra);
            Ok(MethodOutcome::grew(Value::None, n))
        }
        "insert" => {
            let [idx_v, byte_v] = args else {
                return Err(InterpreterError::TypeError(
                    "insert() takes exactly two arguments".into(),
                )
                .into());
            };
            let byte = byte_arg(byte_v)?;
            let mut b = ba.lock();
            let len = b.len() as i64;
            let raw = match idx_v {
                Value::Int(n) => *n,
                Value::Bool(bo) => i64::from(*bo),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "an integer is required for the index".into(),
                    )
                    .into());
                }
            };
            // CPython clamps insert index into [0, len].
            let idx = if raw < 0 { (len + raw).max(0) } else { raw.min(len) };
            b.insert(usize::try_from(idx).unwrap_or(0), byte);
            Ok(MethodOutcome::grew(Value::None, 1))
        }
        "remove" => {
            let byte = byte_arg(args.first().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError("remove() takes one argument".into()))
            })?)?;
            let mut b = ba.lock();
            match b.iter().position(|&x| x == byte) {
                Some(pos) => {
                    b.remove(pos);
                    Ok(MethodOutcome::shrank(Value::None, 1))
                }
                None => {
                    Err(EvalError::Exception(ExceptionValue::new("ValueError", "value not found")))
                }
            }
        }
        "pop" => {
            let mut b = ba.lock();
            if b.is_empty() {
                return Err(EvalError::Exception(ExceptionValue::new(
                    "IndexError",
                    "pop from empty bytearray",
                )));
            }
            let len = b.len() as i64;
            let raw = match args.first() {
                None => len - 1,
                Some(Value::Int(n)) => *n,
                Some(Value::Bool(bo)) => i64::from(*bo),
                Some(_) => {
                    return Err(InterpreterError::TypeError("an integer is required".into()).into());
                }
            };
            let idx = if raw < 0 { len + raw } else { raw };
            if idx < 0 || idx >= len {
                return Err(EvalError::Exception(ExceptionValue::new(
                    "IndexError",
                    "bytearray index out of range",
                )));
            }
            let removed = b.remove(usize::try_from(idx).unwrap_or(0));
            Ok(MethodOutcome::shrank(Value::Int(i64::from(removed)), 1))
        }
        "clear" => {
            let freed = ba.lock().len();
            ba.lock().clear();
            Ok(MethodOutcome::shrank(Value::None, freed))
        }
        "reverse" => {
            ba.lock().reverse();
            Ok(MethodOutcome::pure(Value::None))
        }
        "copy" => Ok(MethodOutcome::pure(Value::ByteArray(shared_bytes(ba.lock().clone())))),
        // Non-mutating: delegate to the bytes implementation on a snapshot,
        // then re-wrap any bytes result as a bytearray.
        _ => {
            let snapshot = ba.lock().clone();
            let result = dispatch_bytes_method(&snapshot, method, args, kwargs)?;
            Ok(MethodOutcome::pure(rewrap_bytes_as_bytearray(result)))
        }
    }
}

/// Dispatch a method on a `memoryview` — `tobytes`, `tolist`, `hex`. `raw` is
/// the underlying buffer's current bytes.
pub(crate) fn dispatch_memoryview_method(raw: &[u8], method: &str) -> EvalResult {
    match method {
        "tobytes" => Ok(Value::Bytes(raw.to_vec())),
        "tolist" => {
            Ok(Value::List(shared_list(raw.iter().map(|&n| Value::Int(i64::from(n))).collect())))
        }
        "hex" => bytes_to_hex(raw, &[]),
        _ => Err(InterpreterError::AttributeError(format!(
            "'memoryview' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// `bytes.hex([sep[, bytes_per_sep]])`. With no separator, returns a bare
/// lowercase hex string. With a one-character `sep`, groups every
/// `bytes_per_sep` bytes (default 1) and joins the groups with `sep`;
/// a positive count groups from the right, a negative count from the left
/// (CPython semantics).
fn bytes_to_hex(raw: &[u8], args: &[Value]) -> EvalResult {
    use std::fmt::Write as _;

    let sep = match args.first() {
        None | Some(Value::None) => None,
        Some(Value::String(s)) => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => {
                    return Err(InterpreterError::ValueError("sep must be length 1.".into()).into());
                }
            }
        }
        Some(other) => {
            return Err(InterpreterError::TypeError(format!(
                "hex() argument 'sep' must be str, not {}",
                other.type_name()
            ))
            .into());
        }
    };

    let Some(sep) = sep else {
        let mut out = String::with_capacity(raw.len() * 2);
        for byte in raw {
            let _ = write!(out, "{byte:02x}");
        }
        return Ok(Value::String(out.into()));
    };

    let group = match args.get(1) {
        None => 1_i64,
        Some(Value::Int(n)) => *n,
        Some(Value::Bool(b)) => i64::from(*b),
        Some(other) => {
            return Err(InterpreterError::TypeError(format!(
                "hex() argument 'bytes_per_sep' must be int, not {}",
                other.type_name()
            ))
            .into());
        }
    };
    if group == 0 {
        return Err(InterpreterError::ValueError("bytes_per_sep must not be zero".into()).into());
    }

    // Group size is |group|; a positive count counts groups from the right
    // (the trailing group may be short), a negative one from the left.
    let width = group.unsigned_abs() as usize;
    let from_right = group > 0;
    let n = raw.len();
    let mut out = String::with_capacity(n * 2 + n / width);
    for (i, byte) in raw.iter().enumerate() {
        if i > 0 {
            let boundary = if from_right { (n - i) % width == 0 } else { i % width == 0 };
            if boundary {
                out.push(sep);
            }
        }
        let _ = write!(out, "{byte:02x}");
    }
    Ok(Value::String(out.into()))
}

/// Convert a `bytes`-returning result of a shared bytes method into the
/// `bytearray` a `bytearray` method returns (recursively for `split`'s list).
fn rewrap_bytes_as_bytearray(value: Value) -> Value {
    match value {
        Value::Bytes(b) => Value::ByteArray(shared_bytes(b)),
        Value::List(items) => {
            let mapped: Vec<Value> = items
                .lock()
                .iter()
                .map(|v| match v {
                    Value::Bytes(b) => Value::ByteArray(shared_bytes(b.clone())),
                    other => other.clone(),
                })
                .collect();
            Value::List(shared_list(mapped))
        }
        other => other,
    }
}

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
            match encoding.to_ascii_lowercase().as_str() {
                "utf-8" | "utf_8" | "u8" => {
                    let s = std::str::from_utf8(b).map_err(|e| {
                        EvalError::from(InterpreterError::ValueError(format!("invalid utf-8: {e}")))
                    })?;
                    Ok(Value::String(s.into()))
                }
                "ascii" | "us-ascii" => {
                    if b.is_ascii() {
                        // SAFETY-of-logic: all bytes < 128, so this is valid
                        // UTF-8 too; from_utf8 cannot fail here.
                        let s = std::str::from_utf8(b).map_err(|e| {
                            EvalError::from(InterpreterError::ValueError(format!(
                                "invalid ascii: {e}"
                            )))
                        })?;
                        Ok(Value::String(s.into()))
                    } else {
                        Err(EvalError::Exception(ExceptionValue::new(
                            "UnicodeDecodeError",
                            "'ascii' codec can't decode byte",
                        )))
                    }
                }
                // latin-1 maps each byte 1:1 to U+00..U+FF, so it never fails.
                "latin-1" | "latin1" | "iso-8859-1" | "iso8859-1" => {
                    Ok(Value::String(b.iter().map(|&byte| byte as char).collect::<String>().into()))
                }
                // UTF-16: the plain name reads (and strips) a leading BOM to pick
                // byte order, defaulting to little-endian; -le/-be force it.
                "utf-16" | "utf16" | "utf-16-le" | "utf-16le" | "utf_16_le" | "utf-16-be"
                | "utf-16be" | "utf_16_be" => {
                    let lname = encoding.to_ascii_lowercase();
                    let (little_endian, body): (bool, &[u8]) = if lname.contains("be") {
                        (false, b)
                    } else if lname.contains("le") {
                        (true, b)
                    } else {
                        match b {
                            [0xFF, 0xFE, rest @ ..] => (true, rest),
                            [0xFE, 0xFF, rest @ ..] => (false, rest),
                            _ => (true, b),
                        }
                    };
                    let units: Vec<u16> = body
                        .chunks_exact(2)
                        .map(|c| {
                            if little_endian {
                                u16::from_le_bytes([c[0], c[1]])
                            } else {
                                u16::from_be_bytes([c[0], c[1]])
                            }
                        })
                        .collect();
                    String::from_utf16(&units).map(|s| Value::String(s.into())).map_err(|_| {
                        EvalError::from(InterpreterError::ValueError("invalid utf-16 data".into()))
                    })
                }
                other => {
                    Err(InterpreterError::ValueError(format!("unknown encoding: {other}")).into())
                }
            }
        }
        "hex" => bytes_to_hex(b, args),
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
        // Case operations act on ASCII letters only; other bytes pass through
        // (CPython `bytes.upper` is ASCII-only).
        "upper" => Ok(Value::Bytes(b.iter().map(u8::to_ascii_uppercase).collect())),
        "lower" => Ok(Value::Bytes(b.iter().map(u8::to_ascii_lowercase).collect())),
        "swapcase" => Ok(Value::Bytes(
            b.iter()
                .map(|c| {
                    if c.is_ascii_uppercase() {
                        c.to_ascii_lowercase()
                    } else if c.is_ascii_lowercase() {
                        c.to_ascii_uppercase()
                    } else {
                        *c
                    }
                })
                .collect(),
        )),
        "capitalize" => {
            let mut out: Vec<u8> = b.iter().map(u8::to_ascii_lowercase).collect();
            if let Some(first) = out.first_mut() {
                *first = first.to_ascii_uppercase();
            }
            Ok(Value::Bytes(out))
        }
        "title" => {
            let mut out = Vec::with_capacity(b.len());
            let mut start_word = true;
            for &c in b {
                if c.is_ascii_alphabetic() {
                    out.push(if start_word {
                        c.to_ascii_uppercase()
                    } else {
                        c.to_ascii_lowercase()
                    });
                    start_word = false;
                } else {
                    out.push(c);
                    start_word = true;
                }
            }
            Ok(Value::Bytes(out))
        }
        "isdigit" => Ok(Value::Bool(!b.is_empty() && b.iter().all(u8::is_ascii_digit))),
        "isalpha" => Ok(Value::Bool(!b.is_empty() && b.iter().all(u8::is_ascii_alphabetic))),
        "isalnum" => Ok(Value::Bool(!b.is_empty() && b.iter().all(u8::is_ascii_alphanumeric))),
        "isspace" => Ok(Value::Bool(!b.is_empty() && b.iter().all(u8::is_ascii_whitespace))),
        "isupper" => Ok(Value::Bool(
            b.iter().any(u8::is_ascii_uppercase) && !b.iter().any(u8::is_ascii_lowercase),
        )),
        "islower" => Ok(Value::Bool(
            b.iter().any(u8::is_ascii_lowercase) && !b.iter().any(u8::is_ascii_uppercase),
        )),
        "strip" | "lstrip" | "rstrip" => {
            let set = match args.first() {
                None | Some(Value::None) => None,
                Some(Value::Bytes(chars)) => Some(chars.clone()),
                Some(_) => {
                    return Err(InterpreterError::TypeError(
                        "a bytes-like object is required".into(),
                    )
                    .into());
                }
            };
            let strip_it = |c: u8| set.as_ref().map_or(c.is_ascii_whitespace(), |s| s.contains(&c));
            let mut lo = 0usize;
            let mut hi = b.len();
            if method != "rstrip" {
                while lo < hi && strip_it(b[lo]) {
                    lo += 1;
                }
            }
            if method != "lstrip" {
                while hi > lo && strip_it(b[hi - 1]) {
                    hi -= 1;
                }
            }
            Ok(Value::Bytes(b[lo..hi].to_vec()))
        }
        "join" => {
            let items =
                crate::eval::control_flow::iterate_value(args.first().ok_or_else(|| {
                    EvalError::from(InterpreterError::TypeError(
                        "join() takes exactly one argument".into(),
                    ))
                })?)?;
            let mut out = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let Value::Bytes(part) = item else {
                    return Err(InterpreterError::TypeError(format!(
                        "sequence item {i}: expected a bytes-like object, {} found",
                        item.type_name()
                    ))
                    .into());
                };
                if i > 0 {
                    out.extend_from_slice(b);
                }
                out.extend_from_slice(part);
            }
            Ok(Value::Bytes(out))
        }
        "removeprefix" => {
            let Some(Value::Bytes(prefix)) = args.first() else {
                return Err(
                    InterpreterError::TypeError("a bytes-like object is required".into()).into()
                );
            };
            Ok(Value::Bytes(b.strip_prefix(prefix.as_slice()).unwrap_or(b).to_vec()))
        }
        "removesuffix" => {
            let Some(Value::Bytes(suffix)) = args.first() else {
                return Err(
                    InterpreterError::TypeError("a bytes-like object is required".into()).into()
                );
            };
            Ok(Value::Bytes(b.strip_suffix(suffix.as_slice()).unwrap_or(b).to_vec()))
        }
        // `translate(table, delete=b'')` — map each byte through the 256-byte
        // table (None leaves bytes unchanged), dropping any byte in `delete`.
        "translate" => {
            let table = match args.first() {
                None | Some(Value::None) => None,
                Some(Value::Bytes(t)) if t.len() == 256 => Some(t.clone()),
                Some(Value::ByteArray(t)) if t.lock().len() == 256 => Some(t.lock().clone()),
                _ => {
                    return Err(InterpreterError::ValueError(
                        "translation table must be 256 bytes long".into(),
                    )
                    .into());
                }
            };
            let delete: Vec<u8> = match args.get(1) {
                None | Some(Value::None) => Vec::new(),
                Some(Value::Bytes(d)) => d.clone(),
                Some(Value::ByteArray(d)) => d.lock().clone(),
                _ => Vec::new(),
            };
            let out: Vec<u8> = b
                .iter()
                .filter(|byte| !delete.contains(byte))
                .map(|&byte| table.as_ref().map_or(byte, |t| t[byte as usize]))
                .collect();
            Ok(Value::Bytes(out))
        }
        // `partition`/`rpartition(sep)` — split at the first / last occurrence
        // into a `(head, sep, tail)` tuple; no match puts the whole string in
        // head (partition) or tail (rpartition) with two empty pieces.
        "partition" | "rpartition" => {
            let Some(Value::Bytes(sep)) = args.first() else {
                return Err(
                    InterpreterError::TypeError("a bytes-like object is required".into()).into()
                );
            };
            if sep.is_empty() {
                return Err(InterpreterError::ValueError("empty separator".into()).into());
            }
            let found = if method == "partition" {
                b.windows(sep.len()).position(|w| w == sep.as_slice())
            } else {
                b.windows(sep.len()).rposition(|w| w == sep.as_slice())
            };
            let (head, tail): (&[u8], &[u8]) = match found {
                Some(i) => (&b[..i], &b[i + sep.len()..]),
                None if method == "partition" => (b, &[]),
                None => (&[], b),
            };
            let sep_bytes: Vec<u8> = if found.is_some() { sep.clone() } else { Vec::new() };
            Ok(Value::Tuple(vec![
                Value::Bytes(head.to_vec()),
                Value::Bytes(sep_bytes),
                Value::Bytes(tail.to_vec()),
            ]))
        }
        // Padding: `center`/`ljust`/`rjust(width, fillbyte=b' ')`. A short
        // width returns the original unchanged.
        "center" | "ljust" | "rjust" => {
            let width = match args.first() {
                Some(Value::Int(n)) => (*n).max(0) as usize,
                Some(Value::Bool(bo)) => usize::from(*bo),
                _ => {
                    return Err(InterpreterError::TypeError(format!(
                        "{method}() takes an integer width"
                    ))
                    .into());
                }
            };
            let fill = match args.get(1) {
                None => b' ',
                Some(Value::Bytes(f)) if f.len() == 1 => f[0],
                Some(Value::ByteArray(f)) if f.lock().len() == 1 => f.lock()[0],
                _ => {
                    return Err(InterpreterError::TypeError(
                        "fill byte must be a byte string of length 1".into(),
                    )
                    .into());
                }
            };
            if b.len() >= width {
                return Ok(Value::Bytes(b.to_vec()));
            }
            let pad = width - b.len();
            let mut out = Vec::with_capacity(width);
            match method {
                "ljust" => {
                    out.extend_from_slice(b);
                    out.extend(std::iter::repeat_n(fill, pad));
                }
                "rjust" => {
                    out.extend(std::iter::repeat_n(fill, pad));
                    out.extend_from_slice(b);
                }
                _ => {
                    let left = pad / 2;
                    out.extend(std::iter::repeat_n(fill, left));
                    out.extend_from_slice(b);
                    out.extend(std::iter::repeat_n(fill, pad - left));
                }
            }
            Ok(Value::Bytes(out))
        }
        // `zfill(width)` left-pads with b'0' to `width`, keeping a leading
        // sign byte (`b'+'`/`b'-'`) first (`b"-42".zfill(6)` -> `b"-0042"`).
        "zfill" => {
            let width = match args.first() {
                Some(Value::Int(n)) => (*n).max(0) as usize,
                Some(Value::Bool(bo)) => usize::from(*bo),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "zfill() takes an integer width".into(),
                    )
                    .into());
                }
            };
            if b.len() >= width {
                return Ok(Value::Bytes(b.to_vec()));
            }
            let pad = width - b.len();
            let mut out = Vec::with_capacity(width);
            if matches!(b.first(), Some(b'+' | b'-')) {
                out.push(b[0]);
                out.extend(std::iter::repeat_n(b'0', pad));
                out.extend_from_slice(&b[1..]);
            } else {
                out.extend(std::iter::repeat_n(b'0', pad));
                out.extend_from_slice(b);
            }
            Ok(Value::Bytes(out))
        }
        // `splitlines(keepends=False)` — unlike str, bytes only treats the
        // ASCII `\n`, `\r`, and `\r\n` as line boundaries (not \v/\f/\x1c-\x1e).
        "splitlines" => {
            let keepends = args.first().is_some_and(Value::is_truthy);
            let mut lines: Vec<Value> = Vec::new();
            let mut start = 0;
            let mut i = 0;
            while i < b.len() {
                if b[i] == b'\n' || b[i] == b'\r' {
                    let mut end = i + 1;
                    if b[i] == b'\r' && end < b.len() && b[end] == b'\n' {
                        end += 1;
                    }
                    let line_end = if keepends { end } else { i };
                    lines.push(Value::Bytes(b[start..line_end].to_vec()));
                    start = end;
                    i = end;
                } else {
                    i += 1;
                }
            }
            if start < b.len() {
                lines.push(Value::Bytes(b[start..].to_vec()));
            }
            Ok(Value::List(crate::value::shared_list(lines)))
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
