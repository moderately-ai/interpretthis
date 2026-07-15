// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `io.StringIO` methods: write/read/readline/readlines/writelines/getvalue/
//! seek/tell/truncate/close, plus the context-manager protocol. The cursor
//! (`pos`) is a character index; `write` overwrites from the cursor (extending
//! the buffer) and `read`/`readline` advance it, matching CPython.

use indexmap::IndexMap;

use crate::{
    error::{EvalError, InterpreterError},
    eval::functions::method_dispatch::{MethodOutcome, reject_kwargs},
    value::{SharedStringIo, Value},
};

fn require_str<'a>(method: &str, args: &'a [Value]) -> Result<&'a str, EvalError> {
    match args.first() {
        Some(Value::String(s)) => Ok(s.as_str()),
        Some(other) => Err(InterpreterError::TypeError(format!(
            "{method}() argument must be str, not {}",
            other.type_name()
        ))
        .into()),
        None => Err(InterpreterError::TypeError(format!(
            "{method}() takes exactly one argument (0 given)"
        ))
        .into()),
    }
}

/// Optional non-negative size argument (`read(n)` / `readline(n)`); `None` or a
/// negative value means "to end".
fn opt_size(args: &[Value]) -> Option<usize> {
    match args.first() {
        Some(Value::Int(n)) if *n >= 0 => usize::try_from(*n).ok(),
        _ => None,
    }
}

pub(crate) fn dispatch_stringio_method(
    stream: &SharedStringIo,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    match method {
        "write" => {
            reject_kwargs(method, kwargs)?;
            let s = require_str(method, args)?;
            let incoming: Vec<char> = s.chars().collect();
            let written = incoming.len();
            let mut g = stream.lock();
            let mut chars: Vec<char> = g.buf.chars().collect();
            let start = g.pos.min(chars.len());
            // Pad with nothing (StringIO never gaps — pos<=len is maintained by
            // seek clamping) and overwrite/extend from the cursor.
            for (i, c) in incoming.into_iter().enumerate() {
                if start + i < chars.len() {
                    chars[start + i] = c;
                } else {
                    chars.push(c);
                }
            }
            g.buf = chars.into_iter().collect();
            g.pos = start + written;
            Ok(MethodOutcome::grew(Value::Int(written as i64), s.len()))
        }
        "writelines" => {
            reject_kwargs(method, kwargs)?;
            let items =
                crate::eval::control_flow::iterate_value(args.first().unwrap_or(&Value::None))?;
            let mut g = stream.lock();
            let mut chars: Vec<char> = g.buf.chars().collect();
            let mut added = 0usize;
            for item in items {
                let Value::String(s) = item else {
                    return Err(InterpreterError::TypeError(
                        "writelines() argument must be an iterable of str".into(),
                    )
                    .into());
                };
                for c in s.chars() {
                    let p = g.pos;
                    if p < chars.len() {
                        chars[p] = c;
                    } else {
                        chars.push(c);
                    }
                    g.pos += 1;
                    added += c.len_utf8();
                }
            }
            g.buf = chars.into_iter().collect();
            Ok(MethodOutcome::grew(Value::None, added))
        }
        "getvalue" => {
            reject_kwargs(method, kwargs)?;
            let g = stream.lock();
            Ok(MethodOutcome::pure(Value::String(g.buf.clone().into())))
        }
        "read" => {
            reject_kwargs(method, kwargs)?;
            let n = opt_size(args);
            let mut g = stream.lock();
            let chars: Vec<char> = g.buf.chars().collect();
            let start = g.pos.min(chars.len());
            let end = n.map_or(chars.len(), |n| (start + n).min(chars.len()));
            let out: String = chars[start..end].iter().collect();
            g.pos = end;
            Ok(MethodOutcome::pure(Value::String(out.into())))
        }
        "readline" => {
            reject_kwargs(method, kwargs)?;
            let limit = opt_size(args);
            let mut g = stream.lock();
            let chars: Vec<char> = g.buf.chars().collect();
            let start = g.pos.min(chars.len());
            let mut end = start;
            while end < chars.len() {
                let is_nl = chars[end] == '\n';
                end += 1;
                if is_nl {
                    break;
                }
            }
            if let Some(limit) = limit {
                end = end.min(start + limit);
            }
            let out: String = chars[start..end].iter().collect();
            g.pos = end;
            Ok(MethodOutcome::pure(Value::String(out.into())))
        }
        "readlines" => {
            reject_kwargs(method, kwargs)?;
            let mut g = stream.lock();
            let chars: Vec<char> = g.buf.chars().collect();
            let start = g.pos.min(chars.len());
            let mut lines: Vec<Value> = Vec::new();
            let mut i = start;
            let mut line_start = start;
            while i < chars.len() {
                if chars[i] == '\n' {
                    let line: String = chars[line_start..=i].iter().collect();
                    lines.push(Value::String(line.into()));
                    line_start = i + 1;
                }
                i += 1;
            }
            if line_start < chars.len() {
                let line: String = chars[line_start..].iter().collect();
                lines.push(Value::String(line.into()));
            }
            g.pos = chars.len();
            Ok(MethodOutcome::pure(Value::List(crate::value::shared_list(lines))))
        }
        "seek" => {
            reject_kwargs(method, kwargs)?;
            let target = match args.first() {
                Some(Value::Int(n)) if *n >= 0 => usize::try_from(*n).unwrap_or(usize::MAX),
                _ => {
                    return Err(
                        InterpreterError::ValueError("negative seek position".into()).into()
                    );
                }
            };
            let mut g = stream.lock();
            g.pos = target;
            Ok(MethodOutcome::pure(Value::Int(target as i64)))
        }
        "tell" => {
            reject_kwargs(method, kwargs)?;
            let g = stream.lock();
            Ok(MethodOutcome::pure(Value::Int(g.pos as i64)))
        }
        "truncate" => {
            reject_kwargs(method, kwargs)?;
            let mut g = stream.lock();
            let chars: Vec<char> = g.buf.chars().collect();
            let size = opt_size(args).unwrap_or(g.pos).min(chars.len());
            g.buf = chars[..size].iter().collect();
            Ok(MethodOutcome::pure(Value::Int(size as i64)))
        }
        // Text streams are always seekable/writable/readable; close/flush and
        // the context-manager protocol are no-ops over the in-memory buffer.
        "seekable" | "readable" | "writable" => {
            reject_kwargs(method, kwargs)?;
            Ok(MethodOutcome::pure(Value::Bool(true)))
        }
        "flush" | "close" => {
            reject_kwargs(method, kwargs)?;
            Ok(MethodOutcome::pure(Value::None))
        }
        "__enter__" => Ok(MethodOutcome::pure(Value::StringIO(stream.clone()))),
        "__exit__" => Ok(MethodOutcome::pure(Value::Bool(false))),
        _ => Err(InterpreterError::AttributeError(format!(
            "'_io.StringIO' object has no attribute '{method}'"
        ))
        .into()),
    }
}
