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

use indexmap::IndexMap;
use unicode_general_category::{GeneralCategory, get_general_category};

use super::super::{
    bind_method_params, opt_index_arg, reject_kwargs, to_index, to_len_i64, value_to_i64,
};
use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::control_flow::iterate_value,
    value::{ExceptionValue, Value, shared_list},
};

/// CPython `str.isdigit()`: a char with Numeric_Type Decimal or Digit. That is
/// every Unicode decimal digit (general category Nd) plus the Digit-typed
/// presentation forms (superscripts, subscripts, circled/parenthesised digits,
/// Ethiopic units) — but not the Numeric-only forms (fractions, Roman numerals)
/// that only `isnumeric` accepts. The general category can't tell Digit from
/// Numeric within No, so the Digit-typed No ranges are listed explicitly.
fn is_unicode_digit(c: char) -> bool {
    if get_general_category(c) == GeneralCategory::DecimalNumber {
        return true;
    }
    matches!(c,
        '\u{00B2}' | '\u{00B3}' | '\u{00B9}'      // superscript 2, 3, 1
        | '\u{1369}'..='\u{1371}'                 // Ethiopic digit 1–9
        | '\u{2070}' | '\u{2074}'..='\u{2079}'    // superscript 0, 4–9
        | '\u{2080}'..='\u{2089}'                 // subscript 0–9
        | '\u{2460}'..='\u{2468}'                 // circled 1–9
        | '\u{2474}'..='\u{247C}'                 // parenthesised 1–9
        | '\u{2488}'..='\u{2490}'                 // digit 1–9 full stop
        | '\u{24EA}'                              // circled 0
        | '\u{24F5}'..='\u{24FD}'                 // double circled 1–9
        | '\u{24FF}'                              // negative circled 0
        | '\u{2776}'..='\u{277E}'                 // dingbat negative circled 1–9
        | '\u{2780}'..='\u{2788}'                 // dingbat circled sans-serif 1–9
        | '\u{278A}'..='\u{2792}'                 // dingbat negative circled sans-serif 1–9
    )
}

pub(crate) fn dispatch_string_method(
    s: &str,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> EvalResult {
    // CPython 3.12 keyword-accepting str methods (others are positional-only
    // and raise TypeError on kwargs — never silently drop).
    // See CONFORMANCE.md#method-call-kwargs.
    if !kwargs.is_empty()
        && !matches!(method, "split" | "rsplit" | "encode" | "expandtabs" | "splitlines")
    {
        reject_kwargs(method, kwargs)?;
    }

    match method {
        "upper" => Ok(Value::String(s.to_uppercase().into())),
        "lower" => Ok(Value::String(s.to_lowercase().into())),
        "strip" => match strip_chars(method, args)? {
            Some(chars) => Ok(Value::String(s.trim_matches(|c: char| chars.contains(c)).into())),
            None => Ok(Value::String(s.trim().into())),
        },
        "lstrip" => match strip_chars(method, args)? {
            Some(chars) => {
                Ok(Value::String(s.trim_start_matches(|c: char| chars.contains(c)).into()))
            }
            None => Ok(Value::String(s.trim_start().into())),
        },
        "rstrip" => match strip_chars(method, args)? {
            Some(chars) => {
                Ok(Value::String(s.trim_end_matches(|c: char| chars.contains(c)).into()))
            }
            None => Ok(Value::String(s.trim_end().into())),
        },
        "split" => {
            let bound = bind_method_params(method, args, kwargs, &["sep", "maxsplit"])?;
            let maxsplit = coerce_maxsplit(bound[1].as_ref())?;
            match &bound[0] {
                None | Some(Value::None) => {
                    Ok(Value::List(shared_list(split_whitespace_max(s, maxsplit)?)))
                }
                Some(Value::String(sep)) => {
                    if sep.is_empty() {
                        return Err(InterpreterError::ValueError("empty separator".into()).into());
                    }
                    let parts: Vec<Value> = if maxsplit < 0 {
                        s.split(sep.as_str()).map(|p| Value::String(p.into())).collect()
                    } else {
                        let n = to_index(maxsplit + 1)?;
                        s.splitn(n, sep.as_str()).map(|p| Value::String(p.into())).collect()
                    };
                    Ok(Value::List(shared_list(parts)))
                }
                Some(_) => {
                    Err(InterpreterError::TypeError("must be str or None, not other type".into())
                        .into())
                }
            }
        }
        "rsplit" => {
            let bound = bind_method_params(method, args, kwargs, &["sep", "maxsplit"])?;
            let maxsplit = coerce_maxsplit(bound[1].as_ref())?;
            match &bound[0] {
                None | Some(Value::None) => {
                    Ok(Value::List(shared_list(rsplit_whitespace_max(s, maxsplit)?)))
                }
                Some(Value::String(sep)) => {
                    if sep.is_empty() {
                        return Err(InterpreterError::ValueError("empty separator".into()).into());
                    }
                    let parts: Vec<Value> = if maxsplit < 0 {
                        // CPython rsplit with no max returns left-to-right order.
                        s.split(sep.as_str()).map(|p| Value::String(p.into())).collect()
                    } else {
                        let n = to_index(maxsplit + 1)?;
                        let mut parts: Vec<Value> =
                            s.rsplitn(n, sep.as_str()).map(|p| Value::String(p.into())).collect();
                        parts.reverse();
                        parts
                    };
                    Ok(Value::List(shared_list(parts)))
                }
                Some(_) => {
                    Err(InterpreterError::TypeError("must be str or None, not other type".into())
                        .into())
                }
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
            // CPython: str.replace(self, old, new, count=-1, /) — positional-only.
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
            let count = if args.len() == 3 { value_to_i64(&args[2])? } else { -1 };
            if count < 0 {
                Ok(Value::String(s.replace(old, new).into()))
            } else {
                Ok(Value::String(s.replacen(old, new, to_index(count)?).into()))
            }
        }
        "startswith" => string_affix(s, method, args, true),
        "endswith" => string_affix(s, method, args, false),
        // Unicode-aware casefold for the common non-lower surprises:
        // German ß → ss, Greek final sigma. Remaining characters use
        // Rust's to_lowercase (covers most scripts; not a full UCD
        // CaseFolding.txt pass).
        "casefold" => Ok(Value::String(unicode_casefold(s).into())),
        "encode" => {
            // CPython: str.encode(encoding="utf-8", errors="strict").
            // We only support utf-8 (the default); other encodings
            // would need a proper codec table, so they fall through
            // to TypeError matching CPython's "unknown encoding".
            // `errors` is accepted for signature parity and ignored when
            // encoding succeeds (only strict path is implemented).
            let bound = bind_method_params(method, args, kwargs, &["encoding", "errors"])?;
            let encoding = match &bound[0] {
                Some(Value::String(name)) => name.as_str(),
                None => "utf-8",
                Some(_) => {
                    return Err(InterpreterError::TypeError(
                        "encode() argument must be str".into(),
                    )
                    .into());
                }
            };
            match encoding.to_ascii_lowercase().as_str() {
                "utf-8" | "utf_8" | "u8" => Ok(Value::Bytes(s.as_bytes().to_vec())),
                "ascii" | "us-ascii" => {
                    if s.is_ascii() {
                        Ok(Value::Bytes(s.as_bytes().to_vec()))
                    } else {
                        Err(EvalError::Exception(ExceptionValue::new(
                            "UnicodeEncodeError",
                            "'ascii' codec can't encode character",
                        )))
                    }
                }
                "latin-1" | "latin1" | "iso-8859-1" | "iso8859-1" => {
                    let mut out = Vec::with_capacity(s.len());
                    for ch in s.chars() {
                        let u = ch as u32;
                        if u > 0xff {
                            return Err(EvalError::Exception(ExceptionValue::new(
                                "UnicodeEncodeError",
                                "'latin-1' codec can't encode character",
                            )));
                        }
                        out.push(u as u8);
                    }
                    Ok(Value::Bytes(out))
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
            let bound = bind_method_params(method, args, kwargs, &["tabsize"])?;
            let tabsize = match &bound[0] {
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
            let (sub, start, end) = parse_search_args(method, args)?;
            let (start_char, bs, be) = resolve_window(s, start, end);
            match s[bs..be].find(sub) {
                Some(pos) => {
                    Ok(Value::Int(to_len_i64(start_char + s[bs..bs + pos].chars().count())?))
                }
                None => Ok(Value::Int(-1)),
            }
        }
        "rfind" => {
            let (sub, start, end) = parse_search_args(method, args)?;
            let (start_char, bs, be) = resolve_window(s, start, end);
            match s[bs..be].rfind(sub) {
                Some(pos) => {
                    Ok(Value::Int(to_len_i64(start_char + s[bs..bs + pos].chars().count())?))
                }
                None => Ok(Value::Int(-1)),
            }
        }
        "index" => {
            let (sub, start, end) = parse_search_args(method, args)?;
            let (start_char, bs, be) = resolve_window(s, start, end);
            match s[bs..be].find(sub) {
                Some(pos) => {
                    Ok(Value::Int(to_len_i64(start_char + s[bs..bs + pos].chars().count())?))
                }
                None => Err(EvalError::Exception(ExceptionValue::new(
                    "ValueError",
                    "substring not found",
                ))),
            }
        }
        "rindex" => {
            let (sub, start, end) = parse_search_args(method, args)?;
            let (start_char, bs, be) = resolve_window(s, start, end);
            match s[bs..be].rfind(sub) {
                Some(pos) => {
                    Ok(Value::Int(to_len_i64(start_char + s[bs..bs + pos].chars().count())?))
                }
                None => Err(EvalError::Exception(ExceptionValue::new(
                    "ValueError",
                    "substring not found",
                ))),
            }
        }
        "count" => {
            let (sub, start, end) = parse_search_args(method, args)?;
            let (_, bs, be) = resolve_window(s, start, end);
            Ok(Value::Int(to_len_i64(s[bs..be].matches(sub).count())?))
        }
        "isdigit" => Ok(Value::Bool(!s.is_empty() && s.chars().all(is_unicode_digit))),
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
        "splitlines" => {
            // `keepends` (default False, positional or keyword): keep the
            // line-break characters when true.
            let bound = bind_method_params(method, args, kwargs, &["keepends"])?;
            let keepends = match &bound[0] {
                None | Some(Value::None) => false,
                Some(v) => v.is_truthy(),
            };
            Ok(Value::List(shared_list(split_lines(s, keepends))))
        }
        "isidentifier" => Ok(Value::Bool(is_identifier(s))),
        "istitle" => Ok(Value::Bool(is_title(s))),
        "isprintable" => {
            // CPython: printable = not "other" or "separator" category, except
            // ASCII space. Empty string is printable.
            Ok(Value::Bool(s.chars().all(|c| c == ' ' || (!c.is_control() && !c.is_whitespace()))))
        }
        "isascii" => Ok(Value::Bool(s.is_ascii())),
        "isdecimal" => {
            // CPython str.isdecimal(): every char is a Unicode decimal digit
            // (general category Nd) — excludes superscripts, fractions, and
            // Roman numerals that isdigit/isnumeric accept.
            Ok(Value::Bool(
                !s.is_empty()
                    && s.chars().all(|c| get_general_category(c) == GeneralCategory::DecimalNumber),
            ))
        }
        "isnumeric" => {
            // CPython str.isnumeric(): every char has a Unicode numeric value —
            // general categories Nd, Nl (letter numbers, e.g. Roman numerals),
            // or No (other numbers, e.g. fractions and superscripts).
            Ok(Value::Bool(
                !s.is_empty()
                    && s.chars().all(|c| {
                        matches!(
                            get_general_category(c),
                            GeneralCategory::DecimalNumber
                                | GeneralCategory::LetterNumber
                                | GeneralCategory::OtherNumber
                        )
                    }),
            ))
        }
        "translate" => {
            let Some(table) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "translate() takes exactly one argument (0 given)".into(),
                )
                .into());
            };
            translate(s, table)
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'str' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Split `s` into lines on Python's universal line boundaries. When `keepends`
/// is true the terminator is retained on each line.
fn split_lines(s: &str, keepends: bool) -> Vec<Value> {
    let mut out = Vec::new();
    let mut line = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        // CPython line boundaries: \n \r \r\n \v \f \x1c \x1d \x1e \x85    .
        let is_break = matches!(
            c,
            '\n' | '\r'
                | '\u{0b}'
                | '\u{0c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        );
        if is_break {
            if keepends {
                line.push(c);
                if c == '\r' && chars.peek() == Some(&'\n') {
                    line.push(chars.next().unwrap_or('\n'));
                }
            } else if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push(Value::String(std::mem::take(&mut line).into()));
        } else {
            line.push(c);
        }
    }
    if !line.is_empty() {
        out.push(Value::String(line.into()));
    }
    out
}

/// Whether `s` is a valid Python identifier: a non-digit XID_Start (or `_`)
/// followed by XID_Continue characters. Approximated with Rust's
/// alphabetic/alphanumeric classes plus `_`.
fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_alphabetic()) {
        return false;
    }
    chars.all(|c| c == '_' || c.is_alphanumeric())
}

/// Whether `s` is title-cased: every run of cased characters starts with an
/// uppercase letter and the rest are lowercase, and there is at least one
/// cased character.
fn is_title(s: &str) -> bool {
    let mut seen_cased = false;
    let mut prev_cased = false;
    for c in s.chars() {
        if c.is_uppercase() {
            if prev_cased {
                return false;
            }
            seen_cased = true;
            prev_cased = true;
        } else if c.is_lowercase() {
            if !prev_cased {
                return false;
            }
            seen_cased = true;
            prev_cased = true;
        } else {
            prev_cased = false;
        }
    }
    seen_cased
}

/// `str.translate(table)` — map each character through `table`, a dict keyed by
/// Unicode code point. A mapped value of `None` deletes the character; an int
/// value is a target code point; a str value is substituted directly.
fn translate(s: &str, table: &Value) -> EvalResult {
    let Value::Dict(map) = table else {
        return Err(InterpreterError::TypeError(format!(
            "'{}' object is not subscriptable",
            table.type_name()
        ))
        .into());
    };
    let map = map.lock();
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let key = crate::value::ValueKey::Int(i64::from(u32::from(c)));
        match map.get(&key) {
            None => out.push(c),
            Some(Value::None) => {}
            Some(Value::Int(n)) => {
                let code = u32::try_from(*n).ok().and_then(char::from_u32).ok_or_else(|| {
                    EvalError::from(InterpreterError::ValueError(
                        "character mapping must be in range(0x110000)".into(),
                    ))
                })?;
                out.push(code);
            }
            Some(Value::String(rep)) => out.push_str(rep),
            Some(other) => {
                return Err(InterpreterError::TypeError(format!(
                    "character mapping must return integer, None or str, not {}",
                    other.type_name()
                ))
                .into());
            }
        }
    }
    Ok(Value::String(out.into()))
}

/// Optional strip character set: absent or `None` → strip whitespace; a `str`
/// → strip that character set; anything else raises `TypeError` (CPython:
/// "strip arg must be None or str").
fn strip_chars<'a>(method: &str, args: &'a [Value]) -> Result<Option<&'a str>, EvalError> {
    match args.first() {
        None | Some(Value::None) => Ok(None),
        Some(Value::String(chars)) => Ok(Some(chars.as_str())),
        Some(_) => {
            Err(InterpreterError::TypeError(format!("{method} arg must be None or str")).into())
        }
    }
}

/// Coerce an optional `maxsplit` argument: absent or `None` → unlimited (`-1`);
/// otherwise an integer (non-integers raise `TypeError` via `value_to_i64`).
fn coerce_maxsplit(arg: Option<&Value>) -> Result<i64, EvalError> {
    match arg {
        None | Some(Value::None) => Ok(-1),
        Some(v) => value_to_i64(v),
    }
}

/// Parse `(sub, start, end)` for the `find`/`rfind`/`index`/`rindex`/`count`
/// family: the substring must be `str`; `start`/`end` are optional integer (or
/// `None`) char indices. Non-integer bounds raise `TypeError` via `value_to_i64`.
fn parse_search_args<'a>(
    method: &str,
    args: &'a [Value],
) -> Result<(&'a str, Option<i64>, Option<i64>), EvalError> {
    if args.is_empty() || args.len() > 3 {
        return Err(
            InterpreterError::TypeError(format!("{method}() takes at least 1 argument")).into()
        );
    }
    let Value::String(sub) = &args[0] else {
        return Err(InterpreterError::TypeError(format!("{method}() argument must be str")).into());
    };
    Ok((sub.as_str(), opt_index_arg(args.get(1))?, opt_index_arg(args.get(2))?))
}

/// Resolve CPython `start`/`end` char indices to a `(clamped_start_char,
/// byte_start, byte_end)` window over `s`. Negative indices count from the end
/// and clamp at 0; out-of-range indices clamp to the length; an inverted range
/// collapses to empty. The char start is returned so a match's byte offset can
/// be translated back to a char index.
fn resolve_window(s: &str, start: Option<i64>, end: Option<i64>) -> (usize, usize, usize) {
    let char_len = s.chars().count() as i64;
    let clamp = |i: i64| -> i64 {
        let i = if i < 0 { i + char_len } else { i };
        i.clamp(0, char_len)
    };
    let start = clamp(start.unwrap_or(0));
    let end = clamp(end.unwrap_or(char_len)).max(start);
    // start/end are in [0, char_len]; the casts cannot truncate.
    let (start, end) = (start as usize, end as usize);
    (start, char_to_byte(s, start), char_to_byte(s, end))
}

/// Byte offset of the `char_idx`-th character (or `s.len()` past the end).
fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map_or(s.len(), |(b, _)| b)
}

/// Shared `startswith`/`endswith`: honour the `start`/`end` window and accept
/// either a single `str` affix or a tuple of `str` affixes (any-match).
fn string_affix(s: &str, method: &str, args: &[Value], is_start: bool) -> EvalResult {
    if args.is_empty() || args.len() > 3 {
        return Err(
            InterpreterError::TypeError(format!("{method}() takes at least 1 argument")).into()
        );
    }
    let (_, bs, be) = resolve_window(s, opt_index_arg(args.get(1))?, opt_index_arg(args.get(2))?);
    let window = &s[bs..be];
    let test = |affix: &str| {
        if is_start { window.starts_with(affix) } else { window.ends_with(affix) }
    };
    let matched = match &args[0] {
        Value::String(p) => test(p.as_str()),
        Value::Tuple(items) => {
            let mut any = false;
            for it in items {
                let Value::String(p) = it else {
                    return Err(InterpreterError::TypeError(format!(
                        "tuple for {method}() must only contain str"
                    ))
                    .into());
                };
                if test(p.as_str()) {
                    any = true;
                    break;
                }
            }
            any
        }
        _ => {
            return Err(InterpreterError::TypeError(format!(
                "{method}() first arg must be str or a tuple of str"
            ))
            .into());
        }
    };
    Ok(Value::Bool(matched))
}

/// Whitespace split with optional maxsplit, preserving the remainder of the
/// original string (CPython: `"a  b  c".split(maxsplit=1) == ['a', 'b  c']`).
fn split_whitespace_max(s: &str, maxsplit: i64) -> Result<Vec<Value>, EvalError> {
    if maxsplit == 0 {
        return Ok(vec![Value::String(s.into())]);
    }
    let mut parts = Vec::new();
    let mut rest = s.trim_start();
    let mut splits = 0i64;
    while !rest.is_empty() {
        if maxsplit >= 0 && splits >= maxsplit {
            parts.push(Value::String(rest.into()));
            break;
        }
        if let Some(ws) = rest.find(char::is_whitespace) {
            parts.push(Value::String(rest[..ws].into()));
            rest = rest[ws..].trim_start();
            splits += 1;
        } else {
            parts.push(Value::String(rest.into()));
            break;
        }
    }
    if parts.is_empty() && s.chars().all(char::is_whitespace) {
        // Empty / all-whitespace → empty list (CPython).
        return Ok(parts);
    }
    Ok(parts)
}

fn rsplit_whitespace_max(s: &str, maxsplit: i64) -> Result<Vec<Value>, EvalError> {
    if maxsplit < 0 {
        return Ok(s.split_whitespace().map(|p| Value::String(p.into())).collect());
    }
    if maxsplit == 0 {
        return Ok(vec![Value::String(s.into())]);
    }
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let n = words.len();
    let keep = usize::try_from(maxsplit).unwrap_or(n).min(n);
    if keep >= n {
        return Ok(words.into_iter().map(|p| Value::String(p.into())).collect());
    }
    // Remainder is everything before the last `keep` words, preserving
    // original internal whitespace (CPython rsplit semantics).
    let target_word = words[n - keep];
    let mut search_from = 0usize;
    for w in &words[..n - keep] {
        let idx = s[search_from..].find(w).map(|i| search_from + i).unwrap_or(search_from);
        search_from = idx + w.len();
    }
    let rem_end =
        s[search_from..].find(target_word).map(|i| search_from + i).unwrap_or(search_from);
    let remainder = s[..rem_end].trim_end();
    let mut parts = Vec::with_capacity(keep + 1);
    parts.push(Value::String(remainder.into()));
    for w in &words[n - keep..] {
        parts.push(Value::String((*w).into()));
    }
    Ok(parts)
}

/// Approximate Unicode casefold (Python `str.casefold`).
fn unicode_casefold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'ß' | 'ẞ' => out.push_str("ss"),
            'ς' | 'Σ' => out.push('σ'),
            c => out.extend(c.to_lowercase()),
        }
    }
    out
}
