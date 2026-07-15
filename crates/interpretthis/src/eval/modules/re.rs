// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `re` module, backed by the `regex` crate.
//!
//! `findall`, `sub`, and `split` return plain values; `match`/`search`/
//! `fullmatch` return a [`Value::ReMatch`] (or `None`) supporting `.group()`,
//! `.groups()`, `.start()`/`.end()`/`.span()`, and `.groupdict()`, with
//! character (not byte) offsets so spans match Python's `str`-based `re`.
//!
//! Two intentional limits: replacement back-references use the regex crate's
//! `$1`/`${1}` syntax (not Python's `\1`); and the regex crate is a linear-time
//! engine, so Python patterns using backreferences or lookaround do not compile
//! (a deliberate ReDoS-safety choice for a sandbox).

use indexmap::IndexMap;
use regex::Regex;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::modules::arg_str,
    value::{ExceptionValue, MatchGroup, MatchValue, Value, ValueKey, shared_list},
};

/// Whether `re` provides a function named `name`.
pub fn has_function(name: &str) -> bool {
    matches!(
        name,
        "findall"
            | "finditer"
            | "sub"
            | "subn"
            | "split"
            | "match"
            | "search"
            | "fullmatch"
            | "compile"
            | "escape"
    )
}

/// Invoke a `re` function. `sub`/`split` accept their `count`/`maxsplit`
/// arguments positionally or by keyword, so kwargs are threaded through.
pub fn call(func: &str, args: &[Value], kwargs: &IndexMap<String, Value>) -> EvalResult {
    match func {
        "compile" => compile_pattern(func, args, kwargs),
        "escape" => {
            // CPython 3.7+ escapes only the regex special characters; ordinary
            // text (letters, digits, most punctuation) passes through.
            let s = arg_str("escape", args, 0)?;
            const SPECIAL: &str = "()[]{}?*+-|^$\\.&~# \t\n\r\x0b\x0c";
            let mut out = String::with_capacity(s.len());
            for ch in s.chars() {
                if SPECIAL.contains(ch) {
                    out.push('\\');
                }
                out.push(ch);
            }
            Ok(Value::String(out.into()))
        }
        "findall" => findall(args, kwargs),
        "finditer" => finditer(args, kwargs),
        "sub" => sub(func, args, kwargs),
        "subn" => subn(func, args, kwargs),
        "split" => split(func, args, kwargs),
        "match" => {
            anchored_search(
                func, args, kwargs, /* anchor_start */ true, /* anchor_end */ false,
            )
        }
        "fullmatch" => anchored_search(func, args, kwargs, true, true),
        "search" => anchored_search(func, args, kwargs, false, false),
        _ => {
            Err(InterpreterError::AttributeError(format!("module 're' has no attribute '{func}'"))
                .into())
        }
    }
}

/// The `count`/`maxsplit` argument, preserving sign. CPython treats `0` (or an
/// absent value) as "unlimited" but a *negative* value as "zero effect" — the
/// two must be distinguished, so this returns the raw `i64` rather than folding
/// both to 0.
fn count_arg(args: &[Value], pos: usize, kwargs: &IndexMap<String, Value>, key: &str) -> i64 {
    match args.get(pos).or_else(|| kwargs.get(key)) {
        Some(Value::Int(n)) => *n,
        Some(Value::Bool(b)) => i64::from(*b),
        _ => 0,
    }
}

/// Compile a pattern, mapping a syntax error to a Python-style `re.error`.
fn compile(pattern: &str) -> Result<Regex, EvalError> {
    Regex::new(pattern)
        .map_err(|e| EvalError::Exception(ExceptionValue::new("re.error", format!("{e}"))))
}

/// Translate the honoured CPython `re` flag bits to the `regex` crate's global
/// inline-flag prefix (`(?ims)`). IGNORECASE/MULTILINE/DOTALL/VERBOSE map
/// directly; UNICODE is already the default, and LOCALE/DEBUG/ASCII have no
/// clean equivalent, so they are accepted and ignored.
fn flag_prefix(flags: i64) -> String {
    let mut inline = String::new();
    if flags & 2 != 0 {
        inline.push('i'); // IGNORECASE
    }
    if flags & 8 != 0 {
        inline.push('m'); // MULTILINE
    }
    if flags & 16 != 0 {
        inline.push('s'); // DOTALL
    }
    if flags & 64 != 0 {
        inline.push('x'); // VERBOSE
    }
    if inline.is_empty() { String::new() } else { format!("(?{inline})") }
}

/// Compile `pattern` with CPython `re` flags applied as a global inline prefix.
fn compile_with_flags(pattern: &str, flags: i64) -> Result<Regex, EvalError> {
    let prefix = flag_prefix(flags);
    if prefix.is_empty() { compile(pattern) } else { compile(&format!("{prefix}{pattern}")) }
}

/// Read a `flags` argument (positional at `idx`, or the `flags` keyword),
/// defaulting to 0. A bool coerces per Python's int tower.
fn flags_arg(args: &[Value], idx: usize, kwargs: &IndexMap<String, Value>) -> i64 {
    let v = args.get(idx).or_else(|| kwargs.get("flags"));
    match v {
        Some(Value::Int(n)) => *n,
        Some(Value::Bool(b)) => i64::from(*b),
        _ => 0,
    }
}

/// `re.compile(pattern)` — validate the pattern (raising `re.error` on a bad
/// one) and return a compiled [`Value::RePattern`]. The pattern source is
/// stored; the pattern's methods recompile on each call, matching observable
/// behaviour without holding a non-`Clone`/non-`Serialize` engine handle in
/// the `Value` enum.
fn compile_pattern(func: &str, args: &[Value], kwargs: &IndexMap<String, Value>) -> EvalResult {
    let pattern = arg_str(func, args, 0)?;
    // Bake any flags into the stored source as an inline prefix so the pattern's
    // methods (which recompile the stored string) carry them without a separate
    // flags slot on `Value::RePattern`.
    let prefix = flag_prefix(flags_arg(args, 1, kwargs));
    let stored = format!("{prefix}{pattern}");
    // Validate eagerly so `re.compile("(")` raises here, as CPython does,
    // rather than deferring the error to first use.
    compile(&stored)?;
    Ok(Value::RePattern(Box::new(stored)))
}

/// Dispatch a method call on a compiled pattern (`pat.search(s)`, etc.). Each
/// method delegates to the module-level function with the stored pattern
/// spliced in as the leading positional argument.
pub fn dispatch_pattern_method(
    pattern: &str,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> EvalResult {
    match method {
        "match" | "search" | "fullmatch" | "findall" | "finditer" | "sub" | "subn" | "split" => {
            let mut full = Vec::with_capacity(args.len() + 1);
            full.push(Value::String(pattern.to_string().into()));
            full.extend_from_slice(args);
            call(method, &full, kwargs)
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'re.Pattern' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// `re.findall(pattern, string)` — all non-overlapping matches. With no capture
/// group, each match is the whole match; with one group, the group; with
/// several, a tuple of groups.
fn findall(args: &[Value], kwargs: &IndexMap<String, Value>) -> EvalResult {
    let pattern = arg_str("findall", args, 0)?;
    let text = arg_str("findall", args, 1)?;
    let re = compile_with_flags(pattern, flags_arg(args, 2, kwargs))?;
    let group_count = re.captures_len().saturating_sub(1);

    let mut result = Vec::new();
    if group_count == 0 {
        for m in re.find_iter(text) {
            result.push(Value::String(m.as_str().into()));
        }
    } else {
        for caps in re.captures_iter(text) {
            if group_count == 1 {
                result.push(Value::String(group_text(&caps, 1)));
            } else {
                let groups =
                    (1..=group_count).map(|i| Value::String(group_text(&caps, i))).collect();
                result.push(Value::Tuple(groups));
            }
        }
    }
    Ok(Value::List(shared_list(result)))
}

/// `re.finditer(pattern, string)` — an iterator of match objects for every
/// non-overlapping match. Modelled eagerly as a list of `re.Match` values.
fn finditer(args: &[Value], kwargs: &IndexMap<String, Value>) -> EvalResult {
    let pattern = arg_str("finditer", args, 0)?;
    let text = arg_str("finditer", args, 1)?;
    let re = compile_with_flags(pattern, flags_arg(args, 2, kwargs))?;
    let out: Vec<Value> = re
        .captures_iter(text)
        .map(|caps| Value::ReMatch(Box::new(build_match(&caps, &re, text))))
        .collect();
    Ok(Value::List(shared_list(out)))
}

/// `re.sub(pattern, repl, string, count=0)`.
fn sub(func: &str, args: &[Value], kwargs: &IndexMap<String, Value>) -> EvalResult {
    let pattern = arg_str(func, args, 0)?;
    let repl = arg_str(func, args, 1)?;
    let text = arg_str(func, args, 2)?;
    let count = count_arg(args, 3, kwargs, "count");
    // A negative count performs zero replacements (CPython); 0 means replace all.
    if count < 0 {
        return Ok(Value::String(text.into()));
    }
    let re = compile_with_flags(pattern, flags_arg(args, 4, kwargs))?;
    let translated = translate_python_repl(repl);
    let replaced = if count == 0 {
        re.replace_all(text, translated.as_str())
    } else {
        re.replacen(text, usize::try_from(count).unwrap_or(0), translated.as_str())
    };
    Ok(Value::String(replaced.into_owned().into()))
}

/// `re.subn(pattern, repl, string, count=0)` — like `sub`, but returns a
/// `(new_string, number_of_subs_made)` tuple.
fn subn(func: &str, args: &[Value], kwargs: &IndexMap<String, Value>) -> EvalResult {
    let pattern = arg_str(func, args, 0)?;
    let repl = arg_str(func, args, 1)?;
    let text = arg_str(func, args, 2)?;
    let count = count_arg(args, 3, kwargs, "count");
    let re = compile_with_flags(pattern, flags_arg(args, 4, kwargs))?;
    let translated = translate_python_repl(repl);
    let (replaced, made): (String, usize) = if count < 0 {
        (text.to_string(), 0)
    } else if count == 0 {
        let n = re.find_iter(text).count();
        (re.replace_all(text, translated.as_str()).into_owned(), n)
    } else {
        let limit = usize::try_from(count).unwrap_or(0);
        let n = re.find_iter(text).take(limit).count();
        (re.replacen(text, limit, translated.as_str()).into_owned(), n)
    };
    Ok(Value::Tuple(vec![
        Value::String(replaced.into()),
        Value::Int(i64::try_from(made).unwrap_or(i64::MAX)),
    ]))
}

/// Translate Python's regex replacement syntax to the Rust `regex`
/// crate's `$`-prefixed form: `\1`...`\9` -> `${1}`...`${9}`,
/// `\g<name>` -> `$name`, `\g<0>` -> `$0`. The brace form is used so
/// `\1abc` (backref-1 followed by literal `abc`) doesn't get parsed
/// as `$1abc` (capture group named `1abc`). Literal `$` is escaped to
/// `$$`.
fn translate_python_repl(repl: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(repl.len());
    let mut chars = repl.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '$' => out.push_str("$$"),
            '\\' => match chars.peek() {
                Some(&n) if n.is_ascii_digit() => {
                    chars.next();
                    let _ = write!(out, "${{{n}}}");
                }
                Some(&'g') => {
                    chars.next();
                    if matches!(chars.peek(), Some(&'<')) {
                        chars.next();
                        let mut name = String::new();
                        while let Some(&ch) = chars.peek() {
                            if ch == '>' {
                                chars.next();
                                break;
                            }
                            name.push(ch);
                            chars.next();
                        }
                        let _ = write!(out, "${{{name}}}");
                    } else {
                        out.push_str("\\g");
                    }
                }
                Some(&'\\') => {
                    chars.next();
                    out.push('\\');
                }
                Some(&'n') => {
                    chars.next();
                    out.push('\n');
                }
                Some(&'t') => {
                    chars.next();
                    out.push('\t');
                }
                _ => out.push('\\'),
            },
            other => out.push(other),
        }
    }
    out
}

/// `re.split(pattern, string, maxsplit=0)`.
fn split(func: &str, args: &[Value], kwargs: &IndexMap<String, Value>) -> EvalResult {
    let pattern = arg_str(func, args, 0)?;
    let text = arg_str(func, args, 1)?;
    let maxsplit = count_arg(args, 2, kwargs, "maxsplit");
    // A negative maxsplit performs zero splits (CPython) — the whole string is
    // returned as the single element.
    if maxsplit < 0 {
        return Ok(Value::List(shared_list(vec![Value::String(text.into())])));
    }
    let re = compile_with_flags(pattern, flags_arg(args, 3, kwargs))?;
    // CPython interleaves the pattern's captured groups between the pieces:
    // `re.split(r'(\s)', 'a b')` -> ['a', ' ', 'b']. A group that did not
    // participate contributes None. Walk the matches manually (the regex
    // crate's own `split` drops captures). maxsplit == 0 means unlimited.
    let group_count = re.captures_len().saturating_sub(1);
    let limit = usize::try_from(maxsplit).unwrap_or(0);
    let mut parts: Vec<Value> = Vec::new();
    let mut last = 0usize;
    let mut splits = 0usize;
    for caps in re.captures_iter(text) {
        if limit != 0 && splits >= limit {
            break;
        }
        let Some(whole) = caps.get(0) else { continue };
        parts.push(Value::String(text[last..whole.start()].into()));
        for g in 1..=group_count {
            parts.push(caps.get(g).map_or(Value::None, |m| Value::String(m.as_str().into())));
        }
        last = whole.end();
        splits += 1;
    }
    parts.push(Value::String(text[last..].into()));
    Ok(Value::List(shared_list(parts)))
}

/// `re.match`/`re.search`/`re.fullmatch`. Returns a [`Value::ReMatch`] or
/// `Value::None`. `anchor_start`/`anchor_end` select the variant's anchoring
/// (match → start; fullmatch → start+end; search → neither).
fn anchored_search(
    func: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    anchor_start: bool,
    anchor_end: bool,
) -> EvalResult {
    let pattern = arg_str(func, args, 0)?;
    let text = arg_str(func, args, 1)?;
    let re = compile_with_flags(pattern, flags_arg(args, 2, kwargs))?;
    // The leftmost match: correct for `search`; `match`/`fullmatch` then require
    // it to begin at 0 (and span to the end for `fullmatch`).
    let Some(caps) = re.captures(text) else {
        return Ok(Value::None);
    };
    let Some(whole) = caps.get(0) else {
        return Ok(Value::None);
    };
    if (anchor_start && whole.start() != 0) || (anchor_end && whole.end() != text.len()) {
        return Ok(Value::None);
    }
    Ok(Value::ReMatch(Box::new(build_match(&caps, &re, text))))
}

/// Build a [`MatchValue`] from regex captures, converting byte offsets to
/// character offsets (Python's `re` indexes by code point).
fn build_match(caps: &regex::Captures<'_>, re: &Regex, text: &str) -> MatchValue {
    let groups = (0..caps.len())
        .map(|i| {
            caps.get(i).map(|m| MatchGroup {
                text: m.as_str().to_string(),
                start: char_offset(text, m.start()),
                end: char_offset(text, m.end()),
            })
        })
        .collect();
    let mut named = indexmap::IndexMap::new();
    for (index, name) in re.capture_names().enumerate() {
        if let Some(name) = name {
            named.insert(name.to_string(), index);
        }
    }
    MatchValue { groups, named }
}

/// Character offset for a byte offset into `text`.
fn char_offset(text: &str, byte: usize) -> usize {
    text.get(..byte).map_or(byte, |prefix| prefix.chars().count())
}

/// Extract capture group `index`, treating a non-participating group as "".
fn group_text(caps: &regex::Captures<'_>, index: usize) -> compact_str::CompactString {
    caps.get(index).map_or_else(compact_str::CompactString::default, |m| m.as_str().into())
}

// ---------------------------------------------------------------------------
// re.Match method dispatch
// ---------------------------------------------------------------------------

/// Dispatch a method call on a `re.Match` value.
pub fn dispatch_match_method(
    m: &MatchValue,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "group" => match args.len() {
            0 => group_value(m, 0),
            1 => group_by_arg(m, &args[0]),
            // `group(a, b, ...)` returns a tuple of the named groups.
            _ => {
                let mut out = Vec::with_capacity(args.len());
                for arg in args {
                    out.push(group_by_arg(m, arg)?);
                }
                Ok(Value::Tuple(out))
            }
        },
        "groups" => {
            // Groups 1.. ; a non-participating group is the `default` arg (None).
            let default = args.first().cloned().unwrap_or(Value::None);
            let out =
                m.groups.iter().skip(1).map(|g| group_or_default(g.as_ref(), &default)).collect();
            Ok(Value::Tuple(out))
        }
        "groupdict" => {
            let mut map = IndexMap::new();
            for (name, &index) in &m.named {
                let value = m
                    .groups
                    .get(index)
                    .and_then(Option::as_ref)
                    .map_or(Value::None, |g| Value::String(g.text.as_str().into()));
                map.insert(ValueKey::String(name.as_str().into()), value);
            }
            Ok(Value::Dict(crate::value::shared_dict(map)))
        }
        "start" => Ok(Value::Int(group_span(m, args)?.0)),
        "end" => Ok(Value::Int(group_span(m, args)?.1)),
        "span" => {
            let (start, end) = group_span(m, args)?;
            Ok(Value::Tuple(vec![Value::Int(start), Value::Int(end)]))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'re.Match' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// `m.group(arg)` where `arg` is an index or a group name.
fn group_by_arg(m: &MatchValue, arg: &Value) -> EvalResult {
    match arg {
        Value::Int(i) => group_value(m, group_index(*i)?),
        Value::String(name) => {
            let index = *m.named.get(name.as_str()).ok_or_else(|| no_such_group(name.as_str()))?;
            group_value(m, index)
        }
        other => Err(InterpreterError::TypeError(format!(
            "group indices must be integers or strings, not '{}'",
            other.type_name()
        ))
        .into()),
    }
}

/// The text of group `index` (None if it did not participate).
fn group_value(m: &MatchValue, index: usize) -> EvalResult {
    let group = m.groups.get(index).ok_or_else(|| no_such_group(&index.to_string()))?;
    Ok(group_or_default(group.as_ref(), &Value::None))
}

/// The (start, end) char span of the group selected by `args[0]` (default 0);
/// a non-participating group is `(-1, -1)`, as in CPython.
fn group_span(m: &MatchValue, args: &[Value]) -> Result<(i64, i64), EvalError> {
    let index = match args.first() {
        None => 0,
        Some(Value::Int(i)) => group_index(*i)?,
        Some(Value::String(name)) => {
            *m.named.get(name.as_str()).ok_or_else(|| no_such_group(name.as_str()))?
        }
        Some(other) => {
            return Err(InterpreterError::TypeError(format!(
                "group indices must be integers or strings, not '{}'",
                other.type_name()
            ))
            .into());
        }
    };
    let group = m.groups.get(index).ok_or_else(|| no_such_group(&index.to_string()))?;
    Ok(group.as_ref().map_or((-1, -1), |g| {
        (i64::try_from(g.start).unwrap_or(-1), i64::try_from(g.end).unwrap_or(-1))
    }))
}

fn group_or_default(group: Option<&MatchGroup>, default: &Value) -> Value {
    group.map_or_else(|| default.clone(), |g| Value::String(g.text.as_str().into()))
}

/// Convert a (possibly negative) Python group index to a `usize`.
fn group_index(i: i64) -> Result<usize, EvalError> {
    usize::try_from(i).map_err(|_| no_such_group(&i.to_string()))
}

fn no_such_group(name: &str) -> EvalError {
    EvalError::Exception(ExceptionValue::new("IndexError", format!("no such group: {name}")))
}

/// `re` module registration.
pub struct ReModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for ReModule {
    fn name(&self) -> &'static str {
        "re"
    }
    fn constant(&self, name: &str) -> Option<Value> {
        // `re.error` — raised on a bad pattern. Unlike statistics/json it
        // subclasses Exception directly, not ValueError. Stored qualified so
        // the traceback reads `re.error:`; `type(e).__name__` is `error`.
        if name == "error" {
            return Some(Value::ExceptionType("re.error".to_string()));
        }
        // Compilation flags. Values match CPython's `re` constants so bit-ors
        // (`re.I | re.M`) combine correctly; `flag_prefix` honours the ones the
        // `regex` crate can express.
        let flag = match name {
            "A" | "ASCII" => 256,
            "I" | "IGNORECASE" => 2,
            "L" | "LOCALE" => 4,
            "M" | "MULTILINE" => 8,
            "S" | "DOTALL" => 16,
            "X" | "VERBOSE" => 64,
            "U" | "UNICODE" => 32,
            "DEBUG" => 128,
            "NOFLAG" => 0,
            _ => return None,
        };
        Some(Value::Int(flag))
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        kwargs: &IndexMap<String, Value>,
        tools: &crate::tools::Tools,
    ) -> EvalResult {
        // sub/subn accept a callable replacement invoked with each match; that
        // needs the async eval path (the sync `call` only handles string repl).
        if matches!(func, "sub" | "subn")
            && args.get(1).is_some_and(|r| !matches!(r, Value::String(_)))
        {
            return sub_with_callable(state, func, args, kwargs, tools).await;
        }
        call(func, args, kwargs)
    }
}

/// `re.sub`/`re.subn` with a callable replacement: invoke it with the
/// `re.Match` for each non-overlapping match and splice in the returned string.
async fn sub_with_callable(
    state: &mut crate::state::InterpreterState,
    func: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &crate::tools::Tools,
) -> EvalResult {
    let pattern = arg_str(func, args, 0)?;
    let repl = args.get(1).cloned().ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError("sub() missing replacement".into()))
    })?;
    let text = arg_str(func, args, 2)?;
    let count = count_arg(args, 3, kwargs, "count");
    let re = compile(pattern)?;

    let mut out = String::new();
    let mut last = 0usize;
    let mut made: i64 = 0;
    // A negative count performs zero replacements (matching the sync path).
    if count >= 0 {
        for caps in re.captures_iter(text) {
            if count > 0 && made >= count {
                break;
            }
            let Some(whole) = caps.get(0) else { continue };
            out.push_str(&text[last..whole.start()]);
            let match_obj = Value::ReMatch(Box::new(build_match(&caps, &re, text)));
            let replaced = crate::eval::functions::call_value_as_function(
                state,
                &repl,
                &[match_obj],
                &IndexMap::new(),
                tools,
            )
            .await?;
            match replaced {
                Value::String(s) => out.push_str(&s),
                other => {
                    return Err(InterpreterError::TypeError(format!(
                        "expected str instance, {} found",
                        other.type_name()
                    ))
                    .into());
                }
            }
            last = whole.end();
            made += 1;
        }
    }
    out.push_str(&text[last..]);

    if func == "subn" {
        Ok(Value::Tuple(vec![Value::String(out.into()), Value::Int(made)]))
    } else {
        Ok(Value::String(out.into()))
    }
}
