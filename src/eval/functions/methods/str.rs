// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `str` method dispatch — case conversion, stripping, splitting,
//! join, replace, startswith/endswith, encoding, partition, find,
//! count, classification predicates, padding (`center`/`ljust`/
//! `rjust`/`zfill`), `expandtabs`, `removeprefix`/`removesuffix`, etc.
//!
//! The set is the commonly-used surface of CPython's `str` API; rarely
//! used methods (translate tables, encoding aliases) are added on
//! demand pinned by parity probes.

use super::super::{to_index, to_len_i64, value_to_i64};
use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::control_flow::iterate_value,
    value::{ExceptionValue, Value, shared_list},
};

pub(crate) fn dispatch_string_method(s: &str, method: &str, args: &[Value]) -> EvalResult {
    match method {
        "upper" => Ok(Value::String(s.to_uppercase().into())),
        "lower" => Ok(Value::String(s.to_lowercase().into())),
        "strip" => {
            if args.is_empty() {
                Ok(Value::String(s.trim().into()))
            } else if let Value::String(chars) = &args[0] {
                Ok(Value::String(s.trim_matches(|c: char| chars.contains(c)).into()))
            } else {
                Ok(Value::String(s.trim().into()))
            }
        }
        "lstrip" => {
            if args.is_empty() {
                Ok(Value::String(s.trim_start().into()))
            } else if let Value::String(chars) = &args[0] {
                Ok(Value::String(s.trim_start_matches(|c: char| chars.contains(c)).into()))
            } else {
                Ok(Value::String(s.trim_start().into()))
            }
        }
        "rstrip" => {
            if args.is_empty() {
                Ok(Value::String(s.trim_end().into()))
            } else if let Value::String(chars) = &args[0] {
                Ok(Value::String(s.trim_end_matches(|c: char| chars.contains(c)).into()))
            } else {
                Ok(Value::String(s.trim_end().into()))
            }
        }
        "split" => {
            if args.is_empty() {
                // Split on whitespace
                let parts: Vec<Value> =
                    s.split_whitespace().map(|p| Value::String(p.into())).collect();
                Ok(Value::List(shared_list(parts)))
            } else if let Value::String(sep) = &args[0] {
                let maxsplit =
                    if args.len() >= 2 { value_to_i64(&args[1]).unwrap_or(-1) } else { -1 };
                let parts: Vec<Value> = if maxsplit < 0 {
                    s.split(sep.as_str()).map(|p| Value::String(p.into())).collect()
                } else {
                    let n = to_index(maxsplit + 1)?;
                    s.splitn(n, sep.as_str()).map(|p| Value::String(p.into())).collect()
                };
                Ok(Value::List(shared_list(parts)))
            } else {
                let parts: Vec<Value> =
                    s.split_whitespace().map(|p| Value::String(p.into())).collect();
                Ok(Value::List(shared_list(parts)))
            }
        }
        "rsplit" => {
            if args.is_empty() {
                let parts: Vec<Value> =
                    s.split_whitespace().map(|p| Value::String(p.into())).collect();
                Ok(Value::List(shared_list(parts)))
            } else if let Value::String(sep) = &args[0] {
                let maxsplit =
                    if args.len() >= 2 { value_to_i64(&args[1]).unwrap_or(-1) } else { -1 };
                let parts: Vec<Value> = if maxsplit < 0 {
                    s.rsplit(sep.as_str()).map(|p| Value::String(p.into())).collect()
                } else {
                    let n = to_index(maxsplit + 1)?;
                    let mut parts: Vec<Value> =
                        s.rsplitn(n, sep.as_str()).map(|p| Value::String(p.into())).collect();
                    parts.reverse();
                    parts
                };
                Ok(Value::List(shared_list(parts)))
            } else {
                Ok(Value::List(shared_list(
                    s.split_whitespace().map(|p| Value::String(p.into())).collect(),
                )))
            }
        }
        "join" => {
            if args.len() != 1 {
                return Err(InterpreterError::TypeError(
                    "join() takes exactly one argument".into(),
                )
                .into());
            }
            let items = iterate_value(&args[0])?;
            let parts: Result<Vec<compact_str::CompactString>, _> = items
                .into_iter()
                .map(|v| match v {
                    Value::String(s) => Ok(s),
                    _ => Err(EvalError::from(InterpreterError::TypeError(format!(
                        "sequence item: expected str, found '{}'",
                        v.type_name()
                    )))),
                })
                .collect();
            let owned = parts?;
            let str_parts: Vec<&str> =
                owned.iter().map(compact_str::CompactString::as_str).collect();
            Ok(Value::String(str_parts.join(s).into()))
        }
        "replace" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(
                    InterpreterError::TypeError("replace() takes 2 or 3 arguments".into()).into()
                );
            }
            let old = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "replace() argument must be str".into(),
                    )
                    .into());
                }
            };
            let new = match &args[1] {
                Value::String(s) => s.as_str(),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "replace() argument must be str".into(),
                    )
                    .into());
                }
            };
            let count = if args.len() == 3 { value_to_i64(&args[2]).unwrap_or(-1) } else { -1 };
            if count < 0 {
                Ok(Value::String(s.replace(old, new).into()))
            } else {
                Ok(Value::String(s.replacen(old, new, to_index(count)?).into()))
            }
        }
        "startswith" => {
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "startswith() takes at least 1 argument".into(),
                )
                .into());
            }
            let prefix = match &args[0] {
                Value::String(p) => p.as_str(),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "startswith() argument must be str".into(),
                    )
                    .into());
                }
            };
            Ok(Value::Bool(s.starts_with(prefix)))
        }
        "endswith" => {
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "endswith() takes at least 1 argument".into(),
                )
                .into());
            }
            let suffix = match &args[0] {
                Value::String(p) => p.as_str(),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "endswith() argument must be str".into(),
                    )
                    .into());
                }
            };
            Ok(Value::Bool(s.ends_with(suffix)))
        }
        // casefold collapses to lower for ASCII; full Unicode
        // case-folding (eg. ß → ss) is a separate refinement we
        // don't currently model — to_lowercase already handles the
        // vast majority of customer text.
        #[expect(
            clippy::match_same_arms,
            reason = "casefold and lower diverge for non-ASCII Unicode (e.g. ß); kept as separate arms so the divergence can be filled in without re-splitting"
        )]
        "casefold" => Ok(Value::String(s.to_lowercase().into())),
        "encode" => {
            // CPython: str.encode(encoding="utf-8", errors="strict").
            // We only support utf-8 (the default); other encodings
            // would need a proper codec table, so they fall through
            // to TypeError matching CPython's "unknown encoding".
            let encoding = match args.first() {
                Some(Value::String(name)) => name.as_str(),
                None => "utf-8",
                _ => {
                    return Err(InterpreterError::TypeError(
                        "encode() argument must be str".into(),
                    )
                    .into());
                }
            };
            match encoding {
                "utf-8" | "utf_8" | "UTF-8" | "UTF_8" | "ascii" | "ASCII" => {
                    Ok(Value::Bytes(s.as_bytes().to_vec()))
                }
                other => {
                    Err(InterpreterError::ValueError(format!("unknown encoding: {other}")).into())
                }
            }
        }
        "expandtabs" => {
            // CPython default tabsize is 8. Each tab character expands
            // to enough spaces to reach the next multiple of tabsize.
            // Newlines reset the column counter — matching CPython.
            let tabsize = match args.first() {
                Some(Value::Int(n)) => usize::try_from((*n).max(0)).unwrap_or(0),
                Some(Value::Bool(b)) => usize::from(*b),
                Some(_) => {
                    return Err(InterpreterError::TypeError(
                        "expandtabs() argument must be int".into(),
                    )
                    .into());
                }
                None => 8,
            };
            let mut out = String::with_capacity(s.len());
            let mut col = 0usize;
            for c in s.chars() {
                match c {
                    '\t' => {
                        let pad = if tabsize == 0 { 0 } else { tabsize - col % tabsize };
                        for _ in 0..pad {
                            out.push(' ');
                        }
                        col += pad;
                    }
                    '\n' | '\r' => {
                        out.push(c);
                        col = 0;
                    }
                    _ => {
                        out.push(c);
                        col += 1;
                    }
                }
            }
            Ok(Value::String(out.into()))
        }
        "partition" => {
            // CPython: returns (head, sep, tail). When the separator
            // is absent, returns (whole_string, "", "").
            let Value::String(sep) = args.first().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "partition() requires 1 argument".into(),
                ))
            })?
            else {
                return Err(
                    InterpreterError::TypeError("partition() argument must be str".into()).into()
                );
            };
            Ok(Value::Tuple(s.find(sep.as_str()).map_or_else(
                || {
                    vec![
                        Value::String(s.into()),
                        Value::String("".into()),
                        Value::String("".into()),
                    ]
                },
                |idx| {
                    vec![
                        Value::String(s[..idx].into()),
                        Value::String(sep.clone()),
                        Value::String(s[idx + sep.len()..].into()),
                    ]
                },
            )))
        }
        "rpartition" => {
            // CPython: same as partition but searches from the right.
            // Missing separator returns ("", "", whole_string).
            let Value::String(sep) = args.first().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "rpartition() requires 1 argument".into(),
                ))
            })?
            else {
                return Err(InterpreterError::TypeError(
                    "rpartition() argument must be str".into(),
                )
                .into());
            };
            Ok(Value::Tuple(s.rfind(sep.as_str()).map_or_else(
                || {
                    vec![
                        Value::String("".into()),
                        Value::String("".into()),
                        Value::String(s.into()),
                    ]
                },
                |idx| {
                    vec![
                        Value::String(s[..idx].into()),
                        Value::String(sep.clone()),
                        Value::String(s[idx + sep.len()..].into()),
                    ]
                },
            )))
        }
        "removeprefix" => {
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "removeprefix() takes exactly 1 argument".into(),
                )
                .into());
            }
            let Value::String(prefix) = &args[0] else {
                return Err(InterpreterError::TypeError(
                    "removeprefix() argument must be str".into(),
                )
                .into());
            };
            Ok(Value::String(s.strip_prefix(prefix.as_str()).unwrap_or(s).into()))
        }
        "removesuffix" => {
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "removesuffix() takes exactly 1 argument".into(),
                )
                .into());
            }
            let Value::String(suffix) = &args[0] else {
                return Err(InterpreterError::TypeError(
                    "removesuffix() argument must be str".into(),
                )
                .into());
            };
            Ok(Value::String(s.strip_suffix(suffix.as_str()).unwrap_or(s).into()))
        }
        "find" => {
            if args.is_empty() {
                return Err(
                    InterpreterError::TypeError("find() takes at least 1 argument".into()).into()
                );
            }
            let sub = match &args[0] {
                Value::String(p) => p.as_str(),
                _ => {
                    return Err(
                        InterpreterError::TypeError("find() argument must be str".into()).into()
                    );
                }
            };
            match s.find(sub) {
                Some(pos) => Ok(Value::Int(to_len_i64(pos)?)),
                None => Ok(Value::Int(-1)),
            }
        }
        "rfind" => {
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "rfind() takes at least 1 argument".into(),
                )
                .into());
            }
            let sub = match &args[0] {
                Value::String(p) => p.as_str(),
                _ => {
                    return Err(
                        InterpreterError::TypeError("rfind() argument must be str".into()).into()
                    );
                }
            };
            match s.rfind(sub) {
                Some(pos) => Ok(Value::Int(to_len_i64(pos)?)),
                None => Ok(Value::Int(-1)),
            }
        }
        "index" => {
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "index() takes at least 1 argument".into(),
                )
                .into());
            }
            let sub = match &args[0] {
                Value::String(p) => p.as_str(),
                _ => {
                    return Err(
                        InterpreterError::TypeError("index() argument must be str".into()).into()
                    );
                }
            };
            match s.find(sub) {
                Some(pos) => Ok(Value::Int(to_len_i64(pos)?)),
                None => Err(EvalError::Exception(ExceptionValue::new(
                    "ValueError",
                    "substring not found",
                ))),
            }
        }
        "count" => {
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "count() takes at least 1 argument".into(),
                )
                .into());
            }
            let sub = match &args[0] {
                Value::String(p) => p.as_str(),
                _ => {
                    return Err(
                        InterpreterError::TypeError("count() argument must be str".into()).into()
                    );
                }
            };
            Ok(Value::Int(to_len_i64(s.matches(sub).count())?))
        }
        "isdigit" => Ok(Value::Bool(!s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))),
        "isalpha" => Ok(Value::Bool(!s.is_empty() && s.chars().all(char::is_alphabetic))),
        "isalnum" => Ok(Value::Bool(!s.is_empty() && s.chars().all(char::is_alphanumeric))),
        "isspace" => Ok(Value::Bool(!s.is_empty() && s.chars().all(char::is_whitespace))),
        "isupper" => {
            Ok(Value::Bool(s.chars().any(char::is_uppercase) && !s.chars().any(char::is_lowercase)))
        }
        "islower" => {
            Ok(Value::Bool(s.chars().any(char::is_lowercase) && !s.chars().any(char::is_uppercase)))
        }
        "title" => {
            let mut result = String::new();
            let mut capitalize_next = true;
            for ch in s.chars() {
                if ch.is_whitespace() || !ch.is_alphanumeric() {
                    result.push(ch);
                    capitalize_next = true;
                } else if capitalize_next {
                    result.extend(ch.to_uppercase());
                    capitalize_next = false;
                } else {
                    result.extend(ch.to_lowercase());
                }
            }
            Ok(Value::String(result.into()))
        }
        "capitalize" => {
            let mut chars = s.chars();
            let result = chars.next().map_or_else(String::new, |first| {
                let rest: String = chars.flat_map(char::to_lowercase).collect();
                let upper: String = first.to_uppercase().collect();
                format!("{upper}{rest}")
            });
            Ok(Value::String(result.into()))
        }
        "swapcase" => {
            let result: String = s
                .chars()
                .flat_map(|c| {
                    if c.is_uppercase() {
                        c.to_lowercase().collect::<Vec<_>>()
                    } else {
                        c.to_uppercase().collect::<Vec<_>>()
                    }
                })
                .collect();
            Ok(Value::String(result.into()))
        }
        "center" => {
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "center() takes at least 1 argument".into(),
                )
                .into());
            }
            let width = to_index(value_to_i64(&args[0])?)?;
            let fill = if args.len() >= 2 {
                match &args[1] {
                    Value::String(f) => f.chars().next().unwrap_or(' '),
                    _ => ' ',
                }
            } else {
                ' '
            };
            let len = s.chars().count();
            if len >= width {
                Ok(Value::String(s.into()))
            } else {
                let total_pad = width - len;
                let left_pad = total_pad / 2;
                let right_pad = total_pad - left_pad;
                let mut result = String::new();
                for _ in 0..left_pad {
                    result.push(fill);
                }
                result.push_str(s);
                for _ in 0..right_pad {
                    result.push(fill);
                }
                Ok(Value::String(result.into()))
            }
        }
        "ljust" => {
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "ljust() takes at least 1 argument".into(),
                )
                .into());
            }
            let width = to_index(value_to_i64(&args[0])?)?;
            let fill = if args.len() >= 2 {
                match &args[1] {
                    Value::String(f) => f.chars().next().unwrap_or(' '),
                    _ => ' ',
                }
            } else {
                ' '
            };
            let len = s.chars().count();
            if len >= width {
                Ok(Value::String(s.into()))
            } else {
                let mut result = s.to_string();
                for _ in 0..(width - len) {
                    result.push(fill);
                }
                Ok(Value::String(result.into()))
            }
        }
        "rjust" => {
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "rjust() takes at least 1 argument".into(),
                )
                .into());
            }
            let width = to_index(value_to_i64(&args[0])?)?;
            let fill = if args.len() >= 2 {
                match &args[1] {
                    Value::String(f) => f.chars().next().unwrap_or(' '),
                    _ => ' ',
                }
            } else {
                ' '
            };
            let len = s.chars().count();
            if len >= width {
                Ok(Value::String(s.into()))
            } else {
                let mut result = String::new();
                for _ in 0..(width - len) {
                    result.push(fill);
                }
                result.push_str(s);
                Ok(Value::String(result.into()))
            }
        }
        "zfill" => {
            if args.is_empty() {
                return Err(
                    InterpreterError::TypeError("zfill() takes exactly 1 argument".into()).into()
                );
            }
            let width = to_index(value_to_i64(&args[0])?)?;
            let len = s.chars().count();
            if len >= width {
                Ok(Value::String(s.into()))
            } else {
                let (sign, digits) = if s.starts_with('-') || s.starts_with('+') {
                    (&s[..1], &s[1..])
                } else {
                    ("", s)
                };
                let zeros = width - len;
                let mut result = String::from(sign);
                for _ in 0..zeros {
                    result.push('0');
                }
                result.push_str(digits);
                Ok(Value::String(result.into()))
            }
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'str' object has no attribute '{method}'"
        ))
        .into()),
    }
}
