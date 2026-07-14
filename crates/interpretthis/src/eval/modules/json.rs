// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `json` module: `dumps` and `loads`.

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::modules::{json_decode_error, need_arg},
    value::{Value, ValueKey},
};

/// Whether `json` provides a function named `name`.
pub fn has_function(name: &str) -> bool {
    matches!(name, "dumps" | "loads")
}

/// Invoke a `json` function.
pub fn call(func: &str, args: &[Value], kwargs: &IndexMap<String, Value>) -> EvalResult {
    match func {
        "dumps" => {
            let value = need_arg(func, args, 0)?;
            let sort_keys = kwargs.get("sort_keys").is_some_and(Value::is_truthy);
            // CPython's `indent=` accepts None (compact), an int
            // (number of spaces per level), or a string (used
            // verbatim as the per-level indent). Strings as indent
            // are rare in customer code, so we model only the int
            // case; non-int falls back to compact.
            let indent: Option<usize> = match kwargs.get("indent") {
                Some(Value::Int(n)) => Some(usize::try_from((*n).max(0)).unwrap_or(0)),
                Some(Value::Bool(b)) => Some(usize::from(*b)),
                _ => None,
            };
            let mut out = String::new();
            write_json(value, sort_keys, indent, 0, &mut out)?;
            Ok(Value::String(out.into()))
        }
        "loads" => {
            let text = match need_arg(func, args, 0)? {
                Value::String(s) => s.clone(),
                other => {
                    return Err(InterpreterError::TypeError(format!(
                        "the JSON object must be str, not '{}'",
                        other.type_name()
                    ))
                    .into());
                }
            };
            let parsed: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| translate_serde_json_error(&e, &text))?;
            Ok(Value::from_json(parsed))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'json' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// Serialize a value to JSON. With `indent=None`, emit CPython's
/// compact form (`, ` and `: ` separators on a single line). With
/// `indent=Some(N)`, each list/dict element gets its own line
/// prefixed by `N * depth` spaces — matching CPython's
/// `json.dumps(obj, indent=N)` byte-for-byte for the common cases.
fn write_json(
    value: &Value,
    sort_keys: bool,
    indent: Option<usize>,
    depth: usize,
    out: &mut String,
) -> Result<(), EvalError> {
    match value {
        Value::None => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::BigInt(b) => out.push_str(&b.to_string()),
        Value::Float(f) => out.push_str(&float_repr(*f)),
        Value::String(s) => write_json_str(s, out),
        Value::List(items) => {
            // Snapshot the items under the lock — write_json recurses,
            // and we want a stable sequence for the duration of the
            // serialisation.
            let snapshot = items.lock().clone();
            write_seq_json(&snapshot, sort_keys, indent, depth, out)?;
        }
        Value::Tuple(items) => {
            write_seq_json(items, sort_keys, indent, depth, out)?;
        }
        Value::Dict(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return Ok(());
            }
            // Optionally emit keys in sorted order (`json.dumps(..., sort_keys=True)`).
            let mut entries: Vec<(&ValueKey, &Value)> = map.iter().collect();
            if sort_keys {
                entries.sort_by(|a, b| compare_keys_for_sort(a.0, b.0));
            }
            out.push('{');
            if let Some(spaces) = indent {
                let inner = " ".repeat(spaces * (depth + 1));
                let outer = " ".repeat(spaces * depth);
                for (i, (key, val)) in entries.into_iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push('\n');
                    out.push_str(&inner);
                    write_json_str(&json_key(key), out);
                    out.push_str(": ");
                    write_json(val, sort_keys, indent, depth + 1, out)?;
                }
                out.push('\n');
                out.push_str(&outer);
            } else {
                for (i, (key, val)) in entries.into_iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    write_json_str(&json_key(key), out);
                    out.push_str(": ");
                    write_json(val, sort_keys, indent, depth, out)?;
                }
            }
            out.push('}');
        }
        other => {
            return Err(InterpreterError::TypeError(format!(
                "Object of type {} is not JSON serializable",
                other.type_name()
            ))
            .into());
        }
    }
    Ok(())
}

/// Compare two `ValueKey`s for `sort_keys=True` ordering — numeric
/// keys compare numerically, strings lexicographically, mixed types
/// by a deterministic tag order. CPython actually raises TypeError on
/// mixed-type keys; we sort them deterministically instead, which is
/// what the legacy derive did before the `Instance` variant landed.
fn compare_keys_for_sort(a: &ValueKey, b: &ValueKey) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    #[expect(
        clippy::cast_precision_loss,
        reason = "i64 -> f64 lossy for |n| > 2^53; sort_keys ordering is best-effort across mixed numeric types"
    )]
    const fn numeric(k: &ValueKey) -> Option<f64> {
        match k {
            ValueKey::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            ValueKey::Int(i) => Some(*i as f64),
            ValueKey::Float(bits) => Some(f64::from_bits(*bits)),
            _ => None,
        }
    }
    if let (Some(x), Some(y)) = (numeric(a), numeric(b)) {
        return x.partial_cmp(&y).unwrap_or(Ordering::Equal);
    }
    match (a, b) {
        (ValueKey::String(sa), ValueKey::String(sb)) => sa.cmp(sb),
        (ValueKey::None, ValueKey::None) => Ordering::Equal,
        _ => json_key(a).cmp(&json_key(b)),
    }
}

/// JSON object keys are always strings; CPython coerces scalar keys.
fn json_key(key: &ValueKey) -> String {
    match key {
        ValueKey::String(s) => s.to_string(),
        ValueKey::Int(i) => i.to_string(),
        ValueKey::BigInt(i) => i.to_string(),
        ValueKey::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        ValueKey::None => "null".to_string(),
        ValueKey::Float(bits) => float_repr(f64::from_bits(*bits)),
        ValueKey::Ellipsis
        | ValueKey::Complex(..)
        | ValueKey::Tuple(_)
        | ValueKey::Frozenset(_)
        | ValueKey::Instance { .. } => {
            format!("{key}")
        }
    }
}

/// Serialise a `Vec<Value>` as a JSON array. Shared between the List
/// and Tuple arms of `write_json` so the formatting logic isn't
/// duplicated.
fn write_seq_json(
    items: &[Value],
    sort_keys: bool,
    indent: Option<usize>,
    depth: usize,
    out: &mut String,
) -> Result<(), EvalError> {
    if items.is_empty() {
        out.push_str("[]");
        return Ok(());
    }
    out.push('[');
    if let Some(spaces) = indent {
        let inner = " ".repeat(spaces * (depth + 1));
        let outer = " ".repeat(spaces * depth);
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('\n');
            out.push_str(&inner);
            write_json(item, sort_keys, indent, depth + 1, out)?;
        }
        out.push('\n');
        out.push_str(&outer);
    } else {
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            write_json(item, sort_keys, indent, depth, out)?;
        }
    }
    out.push(']');
    Ok(())
}

/// Quote and escape a string the way CPython's `json.dumps` does by default
/// (`ensure_ascii=True`): the short escapes for the standard control chars,
/// `\uXXXX` for other control chars and every non-ASCII code point, and a UTF-16
/// surrogate pair for astral characters.
fn write_json_str(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c => escape_unicode(c, out),
        }
    }
    out.push('"');
}

/// Emit `\uXXXX` for `c`, using a surrogate pair for code points above U+FFFF.
fn escape_unicode(c: char, out: &mut String) {
    use std::fmt::Write;
    let cp = u32::from(c);
    if cp > 0xFFFF {
        let v = cp - 0x10000;
        let high = 0xD800 + (v >> 10);
        let low = 0xDC00 + (v & 0x3FF);
        // Writing to a String is infallible; the Result is intentionally ignored.
        let _ = write!(out, "\\u{high:04x}\\u{low:04x}");
    } else {
        let _ = write!(out, "\\u{cp:04x}");
    }
}

/// Render a float the way CPython's `json` does: integral floats keep `.0`,
/// and the non-finite values use the JS-style spellings `json` emits by default.
fn float_repr(f: f64) -> String {
    if f.is_nan() {
        "NaN".to_string()
    } else if f.is_infinite() {
        if f > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
    } else {
        // `Value::Float`'s Display already prints `1.0`, `2.5`, etc.
        format!("{}", Value::Float(f))
    }
}

/// Translate a serde_json error into CPython's
/// `json.decoder.JSONDecodeError` wording. CPython's format is
/// `Expecting <thing>: line N column M (char K)` where K is the
/// 0-based byte offset of the failure. serde_json reports line +
/// column directly; we map its message text to CPython's expected
/// phrases and compute char K from the input text.
///
/// Coverage targets the common failure modes for LLM-emitted JSON
/// (incomplete object, non-JSON garbage, missing colon/comma). The
/// fallback is `Expecting value` — that's CPython's wording for
/// "couldn't tell what the parser was looking for", which is the
/// most common form anyway.
fn translate_serde_json_error(err: &serde_json::Error, text: &str) -> EvalError {
    let raw = err.to_string();

    // Map serde_json wording → CPython prefix. Substring match on the
    // raw message because serde_json's Error doesn't expose enough
    // structure to classify directly.
    let cpython_prefix = if raw.contains("key must be a string")
        || raw.contains("expected `\"`")
        || raw.contains("EOF while parsing an object")
    {
        "Expecting property name enclosed in double quotes"
    } else if raw.contains("expected `:`") {
        "Expecting ':' delimiter"
    } else if raw.contains("expected `,`") || raw.contains("expected `,` or `]`") {
        "Expecting ',' delimiter"
    } else if raw.contains("trailing comma") {
        "Illegal trailing comma before end of object"
    } else {
        // CPython default for "couldn't tell what was expected" —
        // covers EOF-at-start, unexpected token, etc.
        "Expecting value"
    };

    // serde_json reports the position AFTER the failing token (e.g.
    // column 2 for `not json` because it already advanced past 'n').
    // CPython reports the position OF the failing token (column 1).
    // For the "Expecting value" prefix specifically (unknown-token-
    // at-start), shift back by one column/byte so the rendered text
    // matches CPython byte-for-byte. Other prefixes are
    // structurally-positioned (e.g. after `{`), so serde_json and
    // CPython already agree.
    let (line, column) = if cpython_prefix == "Expecting value" {
        let raw_line = err.line();
        let raw_col = err.column();
        // Don't underflow past column 1.
        let shifted_col = raw_col.saturating_sub(1).max(1);
        (raw_line, shifted_col)
    } else {
        (err.line(), err.column())
    };
    let char_offset = char_offset_at(text, line, column);

    json_decode_error(format!("{cpython_prefix}: line {line} column {column} (char {char_offset})"))
}

/// 0-based byte offset of (1-based line, 1-based column) inside text.
/// Walks until the right line is reached, then adds (column - 1)
/// bytes. Saturates on out-of-bounds so we always return *some*
/// offset rather than panicking on a malformed serde_json position.
fn char_offset_at(text: &str, line: usize, column: usize) -> usize {
    let mut offset = 0usize;
    let mut current_line = 1usize;
    for ch in text.chars() {
        if current_line == line {
            return offset + column.saturating_sub(1);
        }
        offset += ch.len_utf8();
        if ch == '\n' {
            current_line += 1;
        }
    }
    offset + column.saturating_sub(1)
}

/// `json` module registration.
pub struct JsonModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for JsonModule {
    fn name(&self) -> &'static str {
        "json"
    }
    fn constant(&self, name: &str) -> Option<Value> {
        // `json.JSONDecodeError` — raised by `json.loads`. Stored as the
        // fully-qualified `json.decoder.JSONDecodeError` (CPython's traceback
        // and hierarchy name); `type(e).__name__` renders `JSONDecodeError`.
        (name == "JSONDecodeError")
            .then(|| Value::ExceptionType("json.decoder.JSONDecodeError".to_string()))
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        _state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        kwargs: &IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        call(func, args, kwargs)
    }
}
