// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use indexmap::IndexMap;
use rustpython_parser::ast::{self, ConversionFlag, Expr};

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{eval_expr, functions::resolve_proxy},
    state::InterpreterState,
    tools::Tools,
    value::{ExceptionValue, Value, ValueKey},
};

/// Maximum format width — guards against denial-of-service via
/// `f"{x:{1_000_000_000}d}"` where a user-controlled width would
/// otherwise allocate gigabytes of pad characters.
const MAX_FORMAT_WIDTH: i64 = 10_000;

/// Convert an i64 bound (width or precision from a parsed format spec) into
/// a `usize`, clamping negative or oversized values to `default`. The parser
/// and caller already bound these to small positive numbers; this is the
/// defensive conversion at the cast site.
fn spec_usize(n: i64, default: usize) -> usize {
    usize::try_from(n).unwrap_or(default)
}

/// Evaluate an f-string (`JoinedStr` node).
pub async fn eval_joined_str(
    state: &mut InterpreterState,
    node: &ast::ExprJoinedStr,
    tools: &Tools,
) -> EvalResult {
    let mut parts = Vec::with_capacity(node.values.len());

    for value in &node.values {
        match value {
            Expr::Constant(c) => {
                // String literal part of the f-string
                parts.push(format!("{}", crate::eval::literals::eval_constant(&c.value)));
            }
            Expr::FormattedValue(fv) => {
                let formatted = eval_formatted_value(state, fv, tools).await?;
                if let Value::String(s) = formatted {
                    parts.push(s.into());
                } else {
                    parts.push(format!("{formatted}"));
                }
            }
            _ => {
                // Fallback: evaluate and stringify
                let result = eval_expr(state, value, tools).await?;
                parts.push(format!("{result}"));
            }
        }
    }

    Ok(Value::String(parts.join("").into()))
}

/// Evaluate a formatted value within an f-string.
pub async fn eval_formatted_value(
    state: &mut InterpreterState,
    node: &ast::ExprFormattedValue,
    tools: &Tools,
) -> EvalResult {
    // Evaluate the expression
    let value = eval_expr(state, &node.value, tools).await?;
    let value = resolve_proxy(&value).await?;

    // Apply conversion flag. `render` is the single state-aware path —
    // f"{p}" on a `@dataclass` instance renders as `Point(x=3, y=4)`
    // rather than `<Point object>`. Run before the format-spec eval
    // reborrows `state` mutably so the borrows don't overlap.
    use crate::eval::render::{RenderMode, render};
    let converted = match node.conversion {
        ConversionFlag::Str => {
            Value::String(render(state, &value, RenderMode::Display, tools).await?.into())
        }
        ConversionFlag::Repr => {
            Value::String(render(state, &value, RenderMode::Repr, tools).await?.into())
        }
        ConversionFlag::Ascii => {
            Value::String(render(state, &value, RenderMode::Ascii, tools).await?.into())
        }
        ConversionFlag::None => value,
    };

    // Apply format spec if present. User-class `__format__` always
    // wins on Instance receivers, even when the spec is empty — that
    // matches CPython, which routes `f"{obj}"` through
    // `obj.__format__("")` before falling back to `str()`. Builtin
    // values keep the existing render → apply_format_spec path.
    if let Some(ref format_spec) = node.format_spec {
        let spec_str = eval_expr(state, format_spec, tools).await?;
        let spec: String = match spec_str {
            Value::String(s) => s.into(),
            other => format!("{other}"),
        };
        if let Some(rendered) = call_format_slot(state, &converted, &spec, tools).await? {
            return Ok(Value::String(rendered.into()));
        }
        if spec.is_empty() {
            Ok(Value::String(render(state, &converted, RenderMode::Display, tools).await?.into()))
        } else {
            apply_format_spec(&converted, &spec)
        }
    } else {
        if let Some(rendered) = call_format_slot(state, &converted, "", tools).await? {
            return Ok(Value::String(rendered.into()));
        }
        Ok(Value::String(render(state, &converted, RenderMode::Display, tools).await?.into()))
    }
}

/// Dispatch `value.__format__(spec)` on a user-class instance. Returns
/// `Ok(Some(s))` when the slot ran (and validated `__format__` returned
/// a str), `Ok(None)` when no slot exists (caller falls through to the
/// builtin rendering path).
pub(crate) async fn call_format_slot(
    state: &mut InterpreterState,
    value: &Value,
    spec: &str,
    tools: &Tools,
) -> Result<Option<String>, EvalError> {
    let Value::Instance(inst) = value else { return Ok(None) };
    let Some((_, method)) =
        crate::eval::classes::lookup_method_in_mro(state, &inst.class_name, "__format__")
    else {
        return Ok(None);
    };
    let spec_arg = Value::String(spec.into());
    let call = crate::eval::functions::CallArgs {
        positional: std::slice::from_ref(&spec_arg),
        keyword: &indexmap::IndexMap::new(),
    };
    let (returned, _self) =
        crate::eval::classes::call_method(state, &method, value.clone(), call, tools).await?;
    match returned {
        Value::String(s) => Ok(Some(s.into())),
        other => Err(crate::error::InterpreterError::TypeError(format!(
            "__format__ must return str, not {}",
            other.type_name()
        ))
        .into()),
    }
}

/// Render a Value according to a parsed format spec, producing the
/// unadorned body (no sign, no padding, no width). The caller wraps this
/// in sign/padding application.
fn format_value_body(
    value: &Value,
    type_char: Option<char>,
    precision: Option<i64>,
    alternate: bool,
) -> Result<String, EvalError> {
    let prec = || spec_usize(precision.unwrap_or(6), 6);
    match (value, type_char) {
        // Ints (i64 or promoted BigInt) share one integer formatter, which
        // raises on an unknown/incompatible code and on an out-of-range `:c`.
        (Value::Int(_) | Value::BigInt(_), _) => {
            format_integer(value, type_char, precision, alternate)
        }
        // A bare bool prints True/False; under a numeric code it is an int 0/1
        // (bool is an int subclass): `f"{True:d}" == "1"`.
        (Value::Bool(b), None) => Ok(if *b { "True" } else { "False" }.to_string()),
        (Value::Bool(b), _) => {
            format_integer(&Value::Int(i64::from(*b)), type_char, precision, alternate)
        }
        (Value::Float(f), Some('f' | 'F') | None) => {
            let p = prec();
            Ok(format!("{f:.p$}"))
        }
        (Value::Float(f), Some('e')) => Ok(format_scientific(*f, prec(), false)),
        (Value::Float(f), Some('E')) => Ok(format_scientific(*f, prec(), true)),
        (Value::Float(f), Some('g' | 'G')) => {
            let p = prec();
            // Use shorter of %e and %f.
            let f_fmt = format!("{f:.p$}");
            let e_fmt = format_scientific(*f, p, type_char == Some('G'));
            Ok(if f_fmt.len() <= e_fmt.len() { f_fmt } else { e_fmt })
        }
        (Value::Float(f), Some('%')) => Ok(format!("{:.*}%", prec(), f * 100.0)),
        // A float under a non-float code (e.g. `:d`, `:x`) is a ValueError.
        (Value::Float(_), Some(c)) => Err(unknown_format_code(c, value)),
        (Value::String(s), _) => Ok(precision.map_or_else(
            || s.to_string(),
            |p| s.chars().take(spec_usize(p, 0)).collect::<String>(),
        )),
        // Remaining types (Decimal, Fraction, None, dates, …) render via
        // Display when no type code is given.
        (_, None) => Ok(format!("{value}")),
        (_, Some(c)) => Err(unknown_format_code(c, value)),
    }
}

/// `ValueError: Unknown format code '<c>' for object of type '<type>'` —
/// CPython's wording for an incompatible presentation code.
fn unknown_format_code(code: char, value: &Value) -> EvalError {
    InterpreterError::ValueError(format!(
        "Unknown format code '{code}' for object of type '{}'",
        value.type_name()
    ))
    .into()
}

/// Format an integer value (`Int`/`BigInt`/coerced `Bool`) under a presentation
/// code. Radix codes preserve the sign and operate on the magnitude (so a
/// promoted BigInt and a negative both render as CPython does); float codes
/// route through the f64 view; `:c` raises `OverflowError` outside
/// `range(0x110000)`; any other code raises `ValueError`.
fn format_integer(
    value: &Value,
    type_char: Option<char>,
    precision: Option<i64>,
    alternate: bool,
) -> Result<String, EvalError> {
    use num_bigint::Sign;
    use num_traits::ToPrimitive as _;

    let big = crate::value::value_as_bigint(value).ok_or_else(|| {
        EvalError::from(InterpreterError::Runtime("expected integer value".into()))
    })?;
    let prec = spec_usize(precision.unwrap_or(6), 6);
    let radix = |kind: char| {
        let mag = big.magnitude();
        let body = match (kind, alternate) {
            ('b', false) => format!("{mag:b}"),
            ('b', true) => format!("{mag:#b}"),
            ('o', false) => format!("{mag:o}"),
            ('o', true) => format!("{mag:#o}"),
            ('x', false) => format!("{mag:x}"),
            ('x', true) => format!("{mag:#x}"),
            ('X', false) => format!("{mag:X}"),
            // Rust's `{:#X}` emits a lowercase `0x` prefix; Python uses `0X`.
            _ => format!("0X{mag:X}"),
        };
        if big.sign() == Sign::Minus { format!("-{body}") } else { body }
    };
    let as_f64 = || big.to_f64().unwrap_or(f64::INFINITY);
    match type_char {
        // `n` is locale-aware in CPython; we render it as plain decimal.
        None | Some('d' | 'n') => Ok(big.to_string()),
        Some('b') => Ok(radix('b')),
        Some('o') => Ok(radix('o')),
        Some('x') => Ok(radix('x')),
        Some('X') => Ok(radix('X')),
        Some('c') => {
            let cp = big.to_u32().and_then(char::from_u32).ok_or_else(|| {
                EvalError::Exception(ExceptionValue::new(
                    "OverflowError",
                    "%c arg not in range(0x110000)",
                ))
            })?;
            Ok(cp.to_string())
        }
        Some('f' | 'F') => Ok(format!("{:.prec$}", as_f64())),
        Some('e') => Ok(format_scientific(as_f64(), prec, false)),
        Some('E') => Ok(format_scientific(as_f64(), prec, true)),
        Some('g' | 'G') => {
            let f = as_f64();
            let f_fmt = format!("{f:.prec$}");
            let e_fmt = format_scientific(f, prec, type_char == Some('G'));
            Ok(if f_fmt.len() <= e_fmt.len() { f_fmt } else { e_fmt })
        }
        Some('%') => Ok(format!("{:.prec$}%", as_f64() * 100.0)),
        Some(c) => Err(unknown_format_code(c, value)),
    }
}

/// Apply a Python-style format spec to a value.
pub(crate) fn apply_format_spec(value: &Value, spec: &str) -> EvalResult {
    // Parse format spec: [[fill]align][sign][#][0][width][grouping_option][.precision][type]
    let chars: Vec<char> = spec.chars().collect();

    if chars.is_empty() {
        return Ok(Value::String(format!("{value}").into()));
    }

    // Detect fill and align
    let (fill, align, rest) = parse_fill_align(&chars);

    // Detect sign
    let (sign, rest) = parse_sign(rest);

    // Detect # (alternate form)
    let (alternate, rest) =
        if !rest.is_empty() && rest[0] == '#' { (true, &rest[1..]) } else { (false, rest) };

    // Detect 0 (zero-padding)
    let (zero_pad, rest) =
        if !rest.is_empty() && rest[0] == '0' { (true, &rest[1..]) } else { (false, rest) };

    // Parse width — cap via module-level MAX_FORMAT_WIDTH to prevent DoS.
    let (width, rest) = parse_number(rest);
    if let Some(w) = width {
        if w > MAX_FORMAT_WIDTH {
            return Err(crate::error::InterpreterError::LimitExceeded(format!(
                "format width {w} exceeds maximum ({MAX_FORMAT_WIDTH})"
            ))
            .into());
        }
    }

    // Parse grouping
    let (grouping, rest) = if !rest.is_empty() && (rest[0] == ',' || rest[0] == '_') {
        (Some(rest[0]), &rest[1..])
    } else {
        (None, rest)
    };

    // Parse precision
    let (precision, rest) = if !rest.is_empty() && rest[0] == '.' {
        let (p, r) = parse_number(&rest[1..]);
        (p, r)
    } else {
        (None, rest)
    };

    // Parse type character
    let type_char = if rest.is_empty() { None } else { Some(rest[0]) };

    // Format the value.
    let raw = format_value_body(value, type_char, precision, alternate)?;
    let formatted = match grouping {
        Some(sep) => apply_thousands_separator(&raw, sep),
        None => raw,
    };

    // Apply sign
    let with_sign = match sign {
        Some('+') => {
            if matches!(value, Value::Int(i) if *i >= 0)
                || matches!(value, Value::Float(f) if *f >= 0.0)
            {
                if formatted.starts_with('-') { formatted } else { format!("+{formatted}") }
            } else {
                formatted
            }
        }
        Some(' ') => {
            if formatted.starts_with('-') {
                formatted
            } else {
                format!(" {formatted}")
            }
        }
        _ => formatted,
    };

    // Apply width and alignment. Width is measured in characters (code points),
    // not UTF-8 bytes — a multi-byte subject counts once per character.
    let width = spec_usize(width.unwrap_or(0), 0);
    let display_width = with_sign.chars().count();
    if display_width >= width {
        return Ok(Value::String(with_sign.into()));
    }

    let fill_char = fill.unwrap_or(if zero_pad { '0' } else { ' ' });
    let padded = match align.unwrap_or(if zero_pad { '=' } else { '<' }) {
        '<' => {
            let padding = width - display_width;
            format!("{with_sign}{}", fill_char.to_string().repeat(padding))
        }
        '>' => {
            let padding = width - display_width;
            format!("{}{with_sign}", fill_char.to_string().repeat(padding))
        }
        '^' => {
            let padding = width - display_width;
            let left = padding / 2;
            let right = padding - left;
            format!(
                "{}{with_sign}{}",
                fill_char.to_string().repeat(left),
                fill_char.to_string().repeat(right)
            )
        }
        '=' => {
            // Padding between sign and digits
            let padding = width - display_width;
            if with_sign.starts_with('-')
                || with_sign.starts_with('+')
                || with_sign.starts_with(' ')
            {
                let (s, rest) = with_sign.split_at(1);
                format!("{s}{}{rest}", fill_char.to_string().repeat(padding))
            } else {
                format!("{}{with_sign}", fill_char.to_string().repeat(padding))
            }
        }
        _ => with_sign,
    };

    Ok(Value::String(padded.into()))
}

/// Insert `sep` every 3 digits in the integer part of `raw`. The
/// integer part is the prefix up to the first `.`, `e`, or `E`; any
/// trailing fraction or exponent passes through unchanged. The sign
/// prefix (`-` or `+`) is preserved so `-1234` -> `-1,234`. Non-numeric
/// strings pass through unchanged so format-spec misuses don't crash
/// the formatter.
fn apply_thousands_separator(raw: &str, sep: char) -> String {
    let (sign, rest) = match raw.as_bytes().first() {
        Some(b'-' | b'+') => (&raw[..1], &raw[1..]),
        _ => ("", raw),
    };
    let int_end = rest.find(['.', 'e', 'E']).unwrap_or(rest.len());
    let (int_part, tail) = rest.split_at(int_end);
    if !int_part.chars().all(|c| c.is_ascii_digit()) {
        return raw.to_string();
    }
    let mut grouped = String::with_capacity(int_part.len() + int_part.len() / 3);
    let bytes = int_part.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            grouped.push(sep);
        }
        grouped.push(*b as char);
    }
    format!("{sign}{grouped}{tail}")
}

fn parse_fill_align(chars: &[char]) -> (Option<char>, Option<char>, &[char]) {
    let aligns = ['<', '>', '^', '='];

    if chars.len() >= 2 && aligns.contains(&chars[1]) {
        (Some(chars[0]), Some(chars[1]), &chars[2..])
    } else if !chars.is_empty() && aligns.contains(&chars[0]) {
        (None, Some(chars[0]), &chars[1..])
    } else {
        (None, None, chars)
    }
}

fn parse_sign(chars: &[char]) -> (Option<char>, &[char]) {
    if !chars.is_empty() && (chars[0] == '+' || chars[0] == '-' || chars[0] == ' ') {
        (Some(chars[0]), &chars[1..])
    } else {
        (None, chars)
    }
}

fn parse_number(chars: &[char]) -> (Option<i64>, &[char]) {
    let mut end = 0;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    if end == 0 {
        (None, chars)
    } else {
        let num_str: String = chars[..end].iter().collect();
        let num = num_str.parse::<i64>().ok();
        (num, &chars[end..])
    }
}

fn format_scientific(val: f64, precision: usize, uppercase: bool) -> String {
    // Rust's `{:e}` emits a bare-digit exponent (`3.14e0`) and elides
    // the sign on positive exponents. CPython always includes the sign
    // and pads the exponent to at least two digits (`3.14e+00`). Parse
    // the exponent out and rebuild in the CPython shape.
    let formatted = format!("{val:.precision$e}");
    let Some(e_idx) = formatted.find('e') else { return formatted };
    let mantissa = &formatted[..e_idx];
    let exp_part = &formatted[e_idx + 1..];
    let (exp_sign, exp_digits) = match exp_part.as_bytes().first() {
        Some(b'-') => ('-', &exp_part[1..]),
        Some(b'+') => ('+', &exp_part[1..]),
        _ => ('+', exp_part),
    };
    let padded_exp =
        if exp_digits.len() < 2 { format!("0{exp_digits}") } else { exp_digits.to_string() };
    let e_char = if uppercase { 'E' } else { 'e' };
    format!("{mantissa}{e_char}{exp_sign}{padded_exp}")
}

// ---------------------------------------------------------------------------
// str.format — replacement-field formatting
// ---------------------------------------------------------------------------

/// Implement `str.format(*args, **kwargs)`.
///
/// Supports auto-numbered `{}`, explicit positional `{0}`, and keyword
/// `{name}` fields, each with an optional `!r`/`!s`/`!a` conversion and a
/// `:format_spec` — the spec is rendered by [`apply_format_spec`], the same
/// engine f-strings use, so there is one formatting code path. `{{` / `}}` are
/// literal-brace escapes. Field names may chain `.attr` (dict-key lookup) and
/// `[idx]`/`[key]` accessors, matching common CPython usage.
pub fn str_format(template: &str, args: &[Value], kwargs: &IndexMap<String, Value>) -> EvalResult {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut auto_index: usize = 0;
    let mut i = 0;
    while i < chars.len() {
        match chars.get(i) {
            Some('{') if chars.get(i + 1) == Some(&'{') => {
                out.push('{');
                i += 2;
            }
            Some('}') if chars.get(i + 1) == Some(&'}') => {
                out.push('}');
                i += 2;
            }
            Some('{') => {
                let mut j = i + 1;
                while j < chars.len() && chars.get(j) != Some(&'}') {
                    j += 1;
                }
                if j >= chars.len() {
                    return Err(InterpreterError::ValueError(
                        "Single '{' encountered in format string".into(),
                    )
                    .into());
                }
                let field: String = chars[i + 1..j].iter().collect();
                out.push_str(&value_text(render_format_field(
                    &field,
                    args,
                    kwargs,
                    &mut auto_index,
                )?));
                i = j + 1;
            }
            Some('}') => {
                return Err(InterpreterError::ValueError(
                    "Single '}' encountered in format string".into(),
                )
                .into());
            }
            Some(other) => {
                out.push(*other);
                i += 1;
            }
            None => break,
        }
    }
    Ok(Value::String(out.into()))
}

/// Extract the display text of a rendered field/conversion value without
/// re-quoting a string (a `Value::String` is emitted verbatim).
fn value_text(value: Value) -> String {
    match value {
        Value::String(s) => s.into(),
        other => format!("{other}"),
    }
}

/// Render one `{...}` replacement field (without the surrounding braces).
fn render_format_field(
    field: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    auto_index: &mut usize,
) -> EvalResult {
    // `field_name[!conversion][:format_spec]` — the spec is everything after the
    // first ':'; the conversion is a single char after a trailing '!'.
    let (head, spec) = match field.split_once(':') {
        Some((h, s)) => (h, Some(s)),
        None => (field, None),
    };
    let (name_part, conversion) = match head.rsplit_once('!') {
        Some((name, conv)) if conv.chars().count() == 1 => (name, conv.chars().next()),
        _ => (head, None),
    };

    let value = resolve_format_arg(name_part, args, kwargs, auto_index)?;

    let converted = match conversion {
        None => value,
        Some('s') => Value::String(format!("{value}").into()),
        Some('r') => Value::String(value.repr().into()),
        Some('a') => Value::String(ascii_repr(&value).into()),
        Some(other) => {
            return Err(InterpreterError::ValueError(format!(
                "Unknown conversion specifier {other}"
            ))
            .into());
        }
    };

    match spec {
        None | Some("") => Ok(Value::String(format!("{converted}").into())),
        Some(s) => apply_format_spec(&converted, s),
    }
}

/// ASCII-escape a value's repr, mirroring the `!a` conversion in f-strings.
fn ascii_repr(value: &Value) -> String {
    value
        .repr()
        .chars()
        .map(|c| if c.is_ascii() { c.to_string() } else { format!("\\u{:04x}", c as u32) })
        .collect()
}

/// Resolve a format field's base selector plus any `.attr` / `[idx]` accessors.
fn resolve_format_arg(
    name_part: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    auto_index: &mut usize,
) -> EvalResult {
    // The base selector runs until the first accessor punctuation.
    let base_end = name_part.find(['.', '[']).unwrap_or(name_part.len());
    let (base, mut rest) = name_part.split_at(base_end);

    let mut current = if base.is_empty() {
        let idx = *auto_index;
        *auto_index += 1;
        args.get(idx).cloned().ok_or_else(|| {
            EvalError_value_error(format!(
                "Replacement index {idx} out of range for positional args tuple"
            ))
        })?
    } else if base.chars().all(|c| c.is_ascii_digit()) {
        let idx: usize = base
            .parse()
            .map_err(|_| EvalError_value_error(format!("invalid positional field '{base}'")))?;
        args.get(idx).cloned().ok_or_else(|| {
            EvalError_value_error(format!(
                "Replacement index {idx} out of range for positional args tuple"
            ))
        })?
    } else {
        kwargs.get(base).cloned().ok_or_else(|| {
            EvalError::Exception(ExceptionValue::new("KeyError", format!("'{base}'")))
        })?
    };

    // Walk `.attr` (dict key) and `[idx]` (int index / dict key) accessors.
    while !rest.is_empty() {
        if let Some(after_dot) = rest.strip_prefix('.') {
            let end = after_dot.find(['.', '[']).unwrap_or(after_dot.len());
            let (attr, tail) = after_dot.split_at(end);
            current = format_get_attr(&current, attr)?;
            rest = tail;
        } else if let Some(after_brk) = rest.strip_prefix('[') {
            let Some(close) = after_brk.find(']') else {
                return Err(EvalError_value_error("expected ']' in format field".into()));
            };
            let key = &after_brk[..close];
            current = format_get_item(&current, key)?;
            rest = &after_brk[close + 1..];
        } else {
            return Err(EvalError_value_error(format!("invalid format field accessor '{rest}'")));
        }
    }

    Ok(current)
}

/// `{name.attr}` — attribute access in a format field. Only dict-keyed access
/// is meaningful for the interpreter's value model.
fn format_get_attr(value: &Value, attr: &str) -> EvalResult {
    match value {
        Value::Dict(map) => map
            .get(&ValueKey::String(attr.into()))
            .cloned()
            .ok_or_else(|| EvalError_value_error(format!("dict has no key '{attr}'"))),
        other => Err(InterpreterError::AttributeError(format!(
            "'{}' object has no attribute '{attr}'",
            other.type_name()
        ))
        .into()),
    }
}

/// `{name[key]}` — item access in a format field. A bare integer indexes a
/// sequence; anything else is a dict string key (CPython does not quote it).
fn format_get_item(value: &Value, key: &str) -> EvalResult {
    if key.chars().all(|c| c.is_ascii_digit()) && !key.is_empty() {
        let idx: usize =
            key.parse().map_err(|_| EvalError_value_error(format!("bad index '{key}'")))?;
        return match value {
            Value::List(items) => items
                .lock()
                .get(idx)
                .cloned()
                .ok_or_else(|| EvalError_value_error("format index out of range".into())),
            Value::Tuple(items) => items
                .get(idx)
                .cloned()
                .ok_or_else(|| EvalError_value_error("format index out of range".into())),
            _ => match value {
                Value::Dict(map) => map
                    .get(&ValueKey::Int(idx_to_i64(idx)?))
                    .cloned()
                    .ok_or_else(|| EvalError_value_error(format!("dict has no key {idx}"))),
                other => Err(InterpreterError::TypeError(format!(
                    "'{}' object is not subscriptable",
                    other.type_name()
                ))
                .into()),
            },
        };
    }
    match value {
        Value::Dict(map) => map
            .get(&ValueKey::String(key.into()))
            .cloned()
            .ok_or_else(|| EvalError_value_error(format!("dict has no key '{key}'"))),
        other => Err(InterpreterError::TypeError(format!(
            "'{}' object is not subscriptable",
            other.type_name()
        ))
        .into()),
    }
}

/// Convert a usize index into i64 for an integer dict key.
fn idx_to_i64(idx: usize) -> Result<i64, EvalError> {
    i64::try_from(idx).map_err(|_| EvalError_value_error("format index overflows i64".into()))
}

/// Build a `ValueError` `EvalError`. Named in `snake_case` deliberately so the
/// many call sites above read as a value-error constructor rather than a type.
#[expect(
    non_snake_case,
    reason = "reads as a ValueError constructor at the dozen format-field call sites; \
              a PascalCase name would imply a type and a snake helper named `value_error` \
              collides with the local `parse_sign`-style verbs"
)]
fn EvalError_value_error(message: String) -> EvalError {
    InterpreterError::ValueError(message).into()
}

// ---------------------------------------------------------------------------
// `%` (printf-style) string formatting
// ---------------------------------------------------------------------------

/// Implement `template % arg`.
///
/// `arg` is spread when it is a tuple, used as a mapping for `%(name)s` fields
/// when it is a dict, and otherwise treated as the single positional value.
/// Each conversion is translated into the `{}`-mini-language and rendered by
/// [`apply_format_spec`] so numeric padding/precision/sign behaviour is shared
/// with f-strings and `str.format`.
pub fn str_percent_format(template: &str, arg: &Value) -> EvalResult {
    let chars: Vec<char> = template.chars().collect();
    let positional: Vec<Value> = match arg {
        Value::Tuple(items) => items.clone(),
        Value::Dict(_) => Vec::new(),
        other => vec![other.clone()],
    };
    let mapping = arg.as_dict();

    let mut out = String::new();
    let mut next_arg = 0usize;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1; // consume '%'
        if chars.get(i) == Some(&'%') {
            out.push('%');
            i += 1;
            continue;
        }

        // Optional `(name)` mapping key.
        let mut mapping_key: Option<String> = None;
        if chars.get(i) == Some(&'(') {
            let mut j = i + 1;
            let mut key = String::new();
            while j < chars.len() && chars[j] != ')' {
                key.push(chars[j]);
                j += 1;
            }
            if j >= chars.len() {
                return Err(InterpreterError::ValueError("incomplete format key".into()).into());
            }
            mapping_key = Some(key);
            i = j + 1;
        }

        // Flags.
        let mut flag_minus = false;
        let mut flag_plus = false;
        let mut flag_space = false;
        let mut flag_zero = false;
        let mut flag_alt = false;
        while let Some(&c) = chars.get(i) {
            match c {
                '-' => flag_minus = true,
                '+' => flag_plus = true,
                ' ' => flag_space = true,
                '0' => flag_zero = true,
                '#' => flag_alt = true,
                _ => break,
            }
            i += 1;
        }

        // Width.
        let mut width = String::new();
        while let Some(&c) = chars.get(i) {
            if c.is_ascii_digit() {
                width.push(c);
                i += 1;
            } else {
                break;
            }
        }

        // Precision.
        let precision: Option<String> = if chars.get(i) == Some(&'.') {
            i += 1;
            let mut p = String::new();
            while let Some(&c) = chars.get(i) {
                if c.is_ascii_digit() {
                    p.push(c);
                    i += 1;
                } else {
                    break;
                }
            }
            Some(p)
        } else {
            None
        };

        // Length modifiers (l, h, L) are accepted and ignored, as in CPython.
        while matches!(chars.get(i), Some('l' | 'h' | 'L')) {
            i += 1;
        }

        let Some(&conv) = chars.get(i) else {
            return Err(InterpreterError::ValueError("incomplete format".into()).into());
        };
        i += 1;

        // Fetch the value this conversion consumes.
        let value = if let Some(ref key) = mapping_key {
            let map = mapping.ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError("format requires a mapping".into()))
            })?;
            map.get(&ValueKey::String(key.as_str().into())).cloned().ok_or_else(|| {
                EvalError::Exception(ExceptionValue::new("KeyError", format!("'{key}'")))
            })?
        } else {
            let v = positional.get(next_arg).cloned().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "not enough arguments for format string".into(),
                ))
            })?;
            next_arg += 1;
            v
        };

        let spec = PercentSpec {
            minus: flag_minus,
            // C printf: `+` (always show sign) takes precedence over ` ` (space
            // for positives). They are mutually exclusive, so one `sign` field.
            sign: if flag_plus {
                Some('+')
            } else if flag_space {
                Some(' ')
            } else {
                None
            },
            zero: flag_zero,
            alt: flag_alt,
            width: parse_opt_i64(&width),
            // `.` with no digits means precision 0 (C printf), `.3` means 3,
            // no `.` at all means "unset".
            precision: precision.as_ref().map(|p| parse_opt_i64(p).unwrap_or(0)),
            conv,
        };
        out.push_str(&value_text(format_percent_conversion(&value, &spec)?));
    }

    // Positional over-supply is a TypeError in CPython ("not all arguments
    // converted"). Mapping form does not consume positionally, so skip then.
    if mapping_key_unused(mapping, next_arg, positional.len()) {
        return Err(InterpreterError::TypeError(
            "not all arguments converted during string formatting".into(),
        )
        .into());
    }

    Ok(Value::String(out.into()))
}

/// Parsed `%`-conversion specifier.
struct PercentSpec {
    /// `-` flag: left-justify.
    minus: bool,
    /// Sign handling for positives: `Some('+')`, `Some(' ')`, or `None`.
    sign: Option<char>,
    /// `0` flag: zero-pad.
    zero: bool,
    /// `#` flag: alternate form.
    alt: bool,
    width: Option<i64>,
    precision: Option<i64>,
    conv: char,
}

fn parse_opt_i64(s: &str) -> Option<i64> {
    if s.is_empty() { None } else { s.parse::<i64>().ok() }
}

/// Whether positional args were left unconsumed (mapping form never is).
const fn mapping_key_unused(
    mapping: Option<&IndexMap<ValueKey, Value>>,
    consumed: usize,
    total: usize,
) -> bool {
    mapping.is_none() && consumed < total
}

/// Render a single `%`-conversion by translating it into the `{}`-spec and
/// delegating to [`apply_format_spec`], coercing the value to the type the
/// conversion expects (e.g. `%d` truncates a float, `%f` widens an int).
fn format_percent_conversion(value: &Value, spec: &PercentSpec) -> EvalResult {
    // Coerce the operand to the type the conversion needs.
    let coerced = match spec.conv {
        // `%c`: an int/bool codepoint or a single-character string. Handled
        // fully here (width/padding via the 's' brace spec) with an early
        // return; out of range raises OverflowError, a float or a multi-char
        // string raises TypeError.
        'c' => {
            use num_traits::ToPrimitive as _;
            let ch = match value {
                Value::String(s) if s.chars().count() == 1 => s.to_string(),
                Value::Int(_) | Value::BigInt(_) | Value::Bool(_) => {
                    crate::value::value_as_bigint(value)
                        .and_then(|b| b.to_u32())
                        .and_then(char::from_u32)
                        .map(|c| c.to_string())
                        .ok_or_else(|| {
                            EvalError::Exception(ExceptionValue::new(
                                "OverflowError",
                                "%c arg not in range(0x110000)",
                            ))
                        })?
                }
                _ => {
                    return Err(
                        InterpreterError::TypeError("%c requires int or char".into()).into()
                    );
                }
            };
            return apply_format_spec(&Value::String(ch.into()), &build_brace_spec(spec, 's'));
        }
        'd' | 'i' | 'u' | 'o' | 'x' | 'X' => match value {
            Value::Int(_) | Value::BigInt(_) => value.clone(),
            Value::Bool(b) => Value::Int(i64::from(*b)),
            Value::Float(f) => Value::Int(percent_trunc(*f)),
            _ => {
                return Err(InterpreterError::TypeError(format!(
                    "%{} format: a number is required, not {}",
                    spec.conv,
                    value.type_name()
                ))
                .into());
            }
        },
        'e' | 'E' | 'f' | 'F' | 'g' | 'G' => Value::Float(value.as_float().ok_or_else(|| {
            EvalError::from(InterpreterError::TypeError(format!(
                "%{} format: a float is required, not {}",
                spec.conv,
                value.type_name()
            )))
        })?),
        's' => Value::String(format!("{value}").into()),
        'r' => Value::String(value.repr().into()),
        _ => {
            return Err(InterpreterError::ValueError(format!(
                "unsupported format character '{}'",
                spec.conv
            ))
            .into());
        }
    };

    let type_char = match spec.conv {
        'i' | 'u' => 'd',
        other => other,
    };
    apply_format_spec(&coerced, &build_brace_spec(spec, type_char))
}

/// Truncate a float toward zero for the integer `%`-conversions.
#[expect(
    clippy::cast_possible_truncation,
    reason = "Python's %d/%x truncate a float operand toward zero before formatting; \
              out-of-range values saturate, matching the lossy C printf semantics"
)]
fn percent_trunc(f: f64) -> i64 {
    f.trunc() as i64
}

/// Translate a parsed `%`-spec into the `{}`-mini-language string consumed by
/// [`apply_format_spec`]. `%` defaults to right-alignment, unlike `{}`.
fn build_brace_spec(spec: &PercentSpec, type_char: char) -> String {
    let mut s = String::new();
    if spec.minus {
        s.push('<');
    } else if !spec.zero {
        // `%` right-aligns by default; `{}` left-aligns, so make it explicit
        // unless zero-padding (handled by the '=' default in apply_format_spec).
        s.push('>');
    }
    if let Some(sign) = spec.sign {
        s.push(sign);
    }
    if spec.alt {
        s.push('#');
    }
    if spec.zero && !spec.minus {
        s.push('0');
    }
    if let Some(w) = spec.width {
        s.push_str(&w.to_string());
    }
    if let Some(p) = spec.precision {
        s.push('.');
        s.push_str(&p.to_string());
    }
    s.push(type_char);
    s
}
