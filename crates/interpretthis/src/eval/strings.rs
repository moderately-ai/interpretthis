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
        // Non-finite floats render as CPython's `nan`/`inf` (lowercase for the
        // lowercase codes, uppercase for F/E/G) rather than Rust's `NaN`/`inf`;
        // the sign is applied by the caller, so return the magnitude form.
        (Value::Float(f), Some(c @ ('f' | 'F' | 'e' | 'E' | 'g' | 'G' | '%')))
            if !f.is_finite() =>
        {
            let base = if f.is_nan() {
                "nan"
            } else if *f < 0.0 {
                "-inf"
            } else {
                "inf"
            };
            let mut out =
                if c.is_ascii_uppercase() { base.to_uppercase() } else { base.to_string() };
            if c == '%' {
                out.push('%');
            }
            Ok(out)
        }
        (Value::Float(f), Some('f' | 'F')) => {
            let p = prec();
            Ok(format!("{f:.p$}"))
        }
        // No presentation type: with no precision, the float keeps its natural
        // repr (`f"{3.14:10}"` is "      3.14", not "3.140000"); with a
        // precision it behaves like the general (`g`) format.
        (Value::Float(f), None) => match precision {
            None => Ok(format!("{value}")),
            Some(p) => {
                let rendered = format_general(*f, spec_usize(p, 6), false, alternate);
                // Unlike `g`, the no-type float keeps at least one fractional
                // digit when fixed notation is used (`f"{1.0:.3}"` is "1.0").
                if rendered.contains(['.', 'e', 'E']) {
                    Ok(rendered)
                } else {
                    Ok(format!("{rendered}.0"))
                }
            }
        },
        (Value::Float(f), Some('e')) => Ok(format_scientific(*f, prec(), false)),
        (Value::Float(f), Some('E')) => Ok(format_scientific(*f, prec(), true)),
        (Value::Float(f), Some('g' | 'G')) => Ok(format_general(
            *f,
            precision.map_or(6, |p| spec_usize(p, 6)),
            type_char == Some('G'),
            alternate,
        )),
        (Value::Float(f), Some('%')) => Ok(format!("{:.*}%", prec(), f * 100.0)),
        // A float under a non-float code (e.g. `:d`, `:x`) is a ValueError.
        (Value::Float(_), Some(c)) => Err(unknown_format_code(c, value)),
        (Value::String(s), _) => Ok(precision.map_or_else(
            || s.to_string(),
            |p| s.chars().take(spec_usize(p, 0)).collect::<String>(),
        )),
        // A complex under a float presentation code formats each part with that
        // code and joins them with the imaginary part's explicit sign, e.g.
        // `f"{3+4j:.2f}"` is "3.00+4.00j".
        (Value::Complex(c), Some('f' | 'F' | 'e' | 'E' | 'g' | 'G' | '%')) => {
            let re = format_value_body(&Value::Float(c.re), type_char, precision, alternate)?;
            let im = format_value_body(&Value::Float(c.im.abs()), type_char, precision, alternate)?;
            let sign = if c.im.is_sign_negative() { "-" } else { "+" };
            Ok(format!("{re}{sign}{im}j"))
        }
        // Decimal supports the float presentation codes. Fixed-point and
        // percent round the exact BigDecimal (so `format(Decimal("2.675"),
        // ".2f")` is "2.68", not the float "2.67"); scientific/general reuse the
        // float formatters (a residual for exact half-even in e/g notation).
        (Value::Decimal(d, _), Some('f' | 'F')) => Ok(format_decimal_fixed(d, prec())),
        (Value::Decimal(d, _), Some('%')) => {
            let scaled = d.as_ref().clone() * bigdecimal::BigDecimal::from(100);
            Ok(format!("{}%", format_decimal_fixed(&scaled, prec())))
        }
        (Value::Decimal(d, _), Some('e' | 'E' | 'g' | 'G')) => {
            use num_traits::ToPrimitive as _;
            let f = d.to_f64().unwrap_or(f64::NAN);
            let body = format_value_body(&Value::Float(f), type_char, precision, alternate)?;
            // Decimal writes the exponent with minimal digits (`e+2`), unlike a
            // float's zero-padded two (`e+02`).
            Ok(minimize_exponent_digits(&body))
        }
        // Remaining types (Fraction, None, dates, …) render via Display when no
        // type code is given.
        (_, None) => Ok(format!("{value}")),
        (_, Some(c)) => Err(unknown_format_code(c, value)),
    }
}

/// Fixed-point Decimal formatting with exact half-even rounding to `precision`
/// fractional digits (`format(Decimal("3.1"), ".3f")` is "3.100").
#[expect(clippy::cast_possible_wrap, reason = "precision is a small spec-bounded value")]
fn format_decimal_fixed(d: &bigdecimal::BigDecimal, precision: usize) -> String {
    d.with_scale_round(precision as i64, bigdecimal::RoundingMode::HalfEven).to_plain_string()
}

/// Trim leading zeros from a float-style exponent (`1.00e+02` -> `1.00e+2`),
/// keeping the sign and at least one digit. `Decimal`'s scientific notation uses
/// minimal exponent digits where a float pads to two; strings without an
/// exponent pass through unchanged.
fn minimize_exponent_digits(s: &str) -> String {
    let Some(epos) = s.find(['e', 'E']) else {
        return s.to_string();
    };
    let (mantissa, exp) = s.split_at(epos);
    // exp is `e`/`E`, then a mandatory sign from the float formatter, then digits.
    if exp.len() < 3 || !matches!(exp.as_bytes().get(1), Some(b'+' | b'-')) {
        return s.to_string();
    }
    let marker = &exp[..1];
    let sign = &exp[1..2];
    let trimmed = exp[2..].trim_start_matches('0');
    let trimmed = if trimmed.is_empty() { "0" } else { trimmed };
    format!("{mantissa}{marker}{sign}{trimmed}")
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

    // CPython forbids a precision with the integer presentation types (and with
    // no type); it is only meaningful for the float codes f/e/g/%.
    if precision.is_some()
        && matches!(type_char, None | Some('d' | 'n' | 'b' | 'o' | 'x' | 'X' | 'c'))
    {
        return Err(InterpreterError::ValueError(
            "Precision not allowed in integer format specifier".into(),
        )
        .into());
    }
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
        Some('g' | 'G') => Ok(format_general(as_f64(), prec, type_char == Some('G'), alternate)),
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

    // IntEnum / IntFlag / StrEnum members format through their mixed-in int/str
    // value (`f"{Priority.HIGH:d}"` == `"10"`), matching CPython where the data
    // type's `__format__` handles the spec. A plain `Enum` has no mixed-in type
    // and keeps the default rendering path below.
    if let Value::EnumMember { value: inner, kind, .. } = value {
        if matches!(
            kind,
            crate::value::EnumKind::Int
                | crate::value::EnumKind::Str
                | crate::value::EnumKind::IntFlag
        ) {
            return apply_format_spec(inner, spec);
        }
    }

    // A date/datetime/time interprets a non-empty spec as a strftime pattern
    // (`f"{d:%Y-%m-%d}"`), not a numeric format spec.
    match value {
        Value::Date(d) => return Ok(Value::String(d.format(spec).to_string().into())),
        Value::DateTime { dt, .. } => {
            return Ok(Value::String(dt.format(spec).to_string().into()));
        }
        Value::Time(t) => return Ok(Value::String(t.format(spec).to_string().into())),
        _ => {}
    }

    // A user-class instance reaching here has no `__format__` slot (the callers
    // dispatch that first) yet was given a non-empty spec. CPython's inherited
    // `object.__format__` rejects exactly this, naming the class:
    // `TypeError: unsupported format string passed to <Class>.__format__`.
    if let Value::Instance(inst) = value {
        return Err(InterpreterError::TypeError(format!(
            "unsupported format string passed to {}.__format__",
            inst.class_name
        ))
        .into());
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

    // Parse type character — it must be the final character of the spec.
    // Anything trailing (e.g. `d.2`, where the type precedes the precision) is
    // a malformed spec, which CPython rejects rather than silently ignoring.
    if rest.len() > 1 {
        return Err(InterpreterError::ValueError(format!(
            "Invalid format specifier '{spec}' for object of type '{}'",
            value.python_type_name()
        ))
        .into());
    }
    let type_char = if rest.is_empty() { None } else { Some(rest[0]) };

    // Format the value.
    let raw = format_value_body(value, type_char, precision, alternate)?;
    let formatted = match grouping {
        Some(sep) => apply_thousands_separator(&raw, sep),
        None => raw,
    };

    // Apply sign
    let with_sign = match sign {
        // `+` forces an explicit sign on any numeric body that isn't already
        // negative — Int/BigInt/Bool/Float/Complex/Decimal/Fraction alike.
        Some('+')
            if matches!(
                value,
                Value::Int(_)
                    | Value::BigInt(_)
                    | Value::Bool(_)
                    | Value::Float(_)
                    | Value::Complex(_)
                    | Value::Decimal(..)
                    | Value::Fraction(_)
            ) =>
        {
            if formatted.starts_with('-') {
                formatted
            } else {
                format!("+{formatted}")
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
    // Default alignment is type-dependent: numeric values right-align, every
    // other value (chiefly str) left-aligns; `0`-padding forces sign-aware `=`.
    let default_align = if zero_pad {
        '='
    } else if matches!(
        value,
        Value::Int(_)
            | Value::BigInt(_)
            | Value::Float(_)
            | Value::Bool(_)
            | Value::Complex(_)
            | Value::Decimal(..)
            | Value::Fraction(_)
    ) {
        '>'
    } else {
        '<'
    };
    let padded = match align.unwrap_or(default_align) {
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
            // Sign-aware padding goes AFTER any sign and AFTER a radix prefix,
            // so `{:#010x}` of 255 is "0x000000ff", not "0000000xff".
            let padding = width - display_width;
            let mut head = 0;
            if matches!(with_sign.as_bytes().first(), Some(b'-' | b'+' | b' ')) {
                head = 1;
            }
            if let Some(after) = with_sign.get(head..head + 2) {
                if matches!(after, "0x" | "0X" | "0o" | "0O" | "0b" | "0B") {
                    head += 2;
                }
            }
            let (prefix, rest) = with_sign.split_at(head);
            format!("{prefix}{}{rest}", fill_char.to_string().repeat(padding))
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

/// Python `g`/`G` general float format. `precision` is the number of
/// significant digits (default 6, minimum 1). Chooses fixed vs scientific by
/// the value's decimal exponent, then — unless `alternate` (`#`) — strips
/// trailing zeros and a trailing decimal point.
fn format_general(val: f64, precision: usize, uppercase: bool, alternate: bool) -> String {
    if val.is_nan() {
        return if uppercase { "NAN".into() } else { "nan".into() };
    }
    if val.is_infinite() {
        let s = if val < 0.0 { "-inf" } else { "inf" };
        return if uppercase { s.to_uppercase() } else { s.into() };
    }
    let p = precision.max(1);
    let mut rendered = if val == 0.0 {
        // Exponent 0 -> fixed with p-1 decimals.
        format!("{val:.*}", p - 1)
    } else {
        // Round to p significant digits, then read the resulting exponent
        // (this accounts for a rounding carry, e.g. 9.99e0 -> 1e1).
        let sci = format!("{val:.*e}", p - 1);
        let exp: i32 = sci.split('e').nth(1).and_then(|e| e.parse().ok()).unwrap_or(0);
        if exp < -4 || exp >= p as i32 {
            format_scientific(val, p - 1, uppercase)
        } else {
            let decimals = usize::try_from(p as i32 - 1 - exp).unwrap_or(0);
            format!("{val:.decimals$}")
        }
    };
    if !alternate {
        rendered = strip_general_zeros(&rendered);
    }
    rendered
}

/// Strip a `g`-format result's trailing zeros and trailing decimal point,
/// leaving any exponent suffix intact (`1.2300e+06` -> `1.23e+06`).
fn strip_general_zeros(s: &str) -> String {
    let (mantissa, exp) = match s.find(['e', 'E']) {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    };
    let trimmed = if mantissa.contains('.') {
        mantissa.trim_end_matches('0').trim_end_matches('.')
    } else {
        mantissa
    };
    format!("{trimmed}{exp}")
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
/// `string.Template.substitute` / `.safe_substitute`. Replaces
/// `$name` / `${name}` from the mapping (positional dict + kwargs, kwargs
/// winning) and `$$` with a literal `$`. In non-safe mode a missing key
/// raises `KeyError` and a malformed placeholder raises `ValueError`; in
/// safe mode both are left in place verbatim.
pub(crate) fn template_substitute(
    template: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    safe: bool,
) -> Result<String, EvalError> {
    // Positional mapping dict (if any), snapshotted.
    let positional: Option<IndexMap<ValueKey, Value>> = match args.first() {
        Some(Value::Dict(map)) => Some(map.lock().clone()),
        _ => None,
    };
    let lookup = |name: &str| -> Option<Value> {
        if let Some(v) = kwargs.get(name) {
            return Some(v.clone());
        }
        positional.as_ref().and_then(|m| m.get(&ValueKey::String(name.into())).cloned())
    };

    let chars: Vec<char> = template.chars().collect();
    let is_ident_start = |c: char| c.is_ascii_alphabetic() || c == '_';
    let is_ident_cont = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(template.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        // A `$` at the very end, `$$`, `${name}`, or `$name`.
        match chars.get(i + 1) {
            Some('$') => {
                out.push('$');
                i += 2;
            }
            Some('{') => {
                let mut j = i + 2;
                while j < chars.len() && chars[j] != '}' {
                    j += 1;
                }
                let name: String = chars[i + 2..j.min(chars.len())].iter().collect();
                let valid = j < chars.len()
                    && !name.is_empty()
                    && name.chars().next().is_some_and(is_ident_start)
                    && name.chars().all(is_ident_cont);
                if valid {
                    match lookup(&name) {
                        Some(v) => out.push_str(&format!("{v}")),
                        None if safe => out.extend(&chars[i..=j]),
                        None => {
                            return Err(EvalError::Exception(ExceptionValue::new(
                                "KeyError",
                                format!("'{name}'"),
                            )));
                        }
                    }
                    i = j + 1;
                } else if safe {
                    out.push('$');
                    i += 1;
                } else {
                    return Err(InterpreterError::ValueError(
                        "Invalid placeholder in string".into(),
                    )
                    .into());
                }
            }
            Some(&c) if is_ident_start(c) => {
                let mut j = i + 1;
                while j < chars.len() && is_ident_cont(chars[j]) {
                    j += 1;
                }
                let name: String = chars[i + 1..j].iter().collect();
                match lookup(&name) {
                    Some(v) => out.push_str(&format!("{v}")),
                    None if safe => out.extend(&chars[i..j]),
                    None => {
                        return Err(EvalError::Exception(ExceptionValue::new(
                            "KeyError",
                            format!("'{name}'"),
                        )));
                    }
                }
                i = j;
            }
            // A lone `$` or `$` before an invalid char.
            _ if safe => {
                out.push('$');
                i += 1;
            }
            _ => {
                return Err(
                    InterpreterError::ValueError("Invalid placeholder in string".into()).into()
                );
            }
        }
    }
    Ok(out)
}

pub async fn str_format(
    state: &mut InterpreterState,
    template: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> EvalResult {
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
                // Scan to the matching '}', tracking nesting depth so a
                // replacement field inside the format spec
                // (`{:>{}}`, `{:.{}f}`) is captured whole rather than
                // truncated at the first inner '}'.
                let mut j = i + 1;
                let mut depth = 1usize;
                while j < chars.len() {
                    match chars.get(j) {
                        Some('{') => depth += 1,
                        Some('}') => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                if j >= chars.len() {
                    return Err(InterpreterError::ValueError(
                        "Single '{' encountered in format string".into(),
                    )
                    .into());
                }
                let field: String = chars[i + 1..j].iter().collect();
                out.push_str(&value_text(
                    render_format_field(state, &field, args, kwargs, &mut auto_index, tools)
                        .await?,
                ));
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
///
/// Boxed future because it and [`resolve_nested_spec`] recurse into each
/// other (a replacement field can appear inside a format spec).
fn render_format_field<'a>(
    state: &'a mut InterpreterState,
    field: &'a str,
    args: &'a [Value],
    kwargs: &'a IndexMap<String, Value>,
    auto_index: &'a mut usize,
    tools: &'a Tools,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = EvalResult> + Send + 'a>> {
    Box::pin(async move {
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

        use crate::eval::render::{RenderMode, render};
        let value = resolve_format_arg(name_part, args, kwargs, auto_index)?;

        let converted = match conversion {
            None => value,
            Some('s') => {
                Value::String(render(state, &value, RenderMode::Display, tools).await?.into())
            }
            Some('r') => {
                Value::String(render(state, &value, RenderMode::Repr, tools).await?.into())
            }
            Some('a') => {
                Value::String(render(state, &value, RenderMode::Ascii, tools).await?.into())
            }
            Some(other) => {
                return Err(InterpreterError::ValueError(format!(
                    "Unknown conversion specifier {other}"
                ))
                .into());
            }
        };

        // Resolve any replacement fields nested inside the spec (`{:>{}}`,
        // `{:.{}f}`) to a literal spec first.
        let resolved_spec: Option<String> = match spec {
            None => None,
            Some(s) if s.contains('{') => {
                Some(resolve_nested_spec(state, s, args, kwargs, auto_index, tools).await?)
            }
            Some(s) => Some(s.to_string()),
        };

        // A user-class `__format__` wins for any spec (empty included), matching
        // CPython's `obj.__format__(spec)`; builtins keep the render/apply path.
        let spec_for_slot = resolved_spec.as_deref().unwrap_or("");
        if let Some(rendered) = call_format_slot(state, &converted, spec_for_slot, tools).await? {
            return Ok(Value::String(rendered.into()));
        }

        match resolved_spec {
            None => Ok(Value::String(
                render(state, &converted, RenderMode::Display, tools).await?.into(),
            )),
            Some(s) if s.is_empty() => Ok(Value::String(
                render(state, &converted, RenderMode::Display, tools).await?.into(),
            )),
            Some(s) => apply_format_spec(&converted, &s),
        }
    })
}

/// Resolve replacement fields nested inside a format spec (one level,
/// as CPython allows). Each `{...}` is rendered to its literal text via
/// [`render_format_field`]; the surrounding spec characters pass
/// through unchanged.
async fn resolve_nested_spec(
    state: &mut InterpreterState,
    spec: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    auto_index: &mut usize,
    tools: &Tools,
) -> Result<String, EvalError> {
    let chars: Vec<char> = spec.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars.get(i) {
            Some('{') => {
                let mut j = i + 1;
                while j < chars.len() && chars.get(j) != Some(&'}') {
                    j += 1;
                }
                if j >= chars.len() {
                    return Err(InterpreterError::ValueError(
                        "unmatched '{' in format spec".into(),
                    )
                    .into());
                }
                let inner: String = chars[i + 1..j].iter().collect();
                out.push_str(&value_text(
                    render_format_field(state, &inner, args, kwargs, auto_index, tools).await?,
                ));
                i = j + 1;
            }
            Some(other) => {
                out.push(*other);
                i += 1;
            }
            None => break,
        }
    }
    Ok(out)
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
            EvalError_index_error(format!(
                "Replacement index {idx} out of range for positional args tuple"
            ))
        })?
    } else if base.chars().all(|c| c.is_ascii_digit()) {
        let idx: usize = base
            .parse()
            .map_err(|_| EvalError_value_error(format!("invalid positional field '{base}'")))?;
        args.get(idx).cloned().ok_or_else(|| {
            EvalError_index_error(format!(
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
    // Gate blocked dunders here too: `{x.__class__}` in an f-string reaches
    // attribute access without passing through `eval_attribute`'s validator.
    // (Today `dispatch_getattr_opt` resolves only builtin type-slots so no
    // blocked name is reachable, but this keeps the format path consistent with
    // every other attribute path.)
    crate::security::validator::validate_attribute(attr)?;
    // A dict field selector reads a string key (the interpreter's value model
    // exposes `{d.key}` as key access rather than CPython's getattr).
    if let Value::Dict(map) = value {
        return map
            .lock()
            .get(&ValueKey::String(attr.into()))
            .cloned()
            .ok_or_else(|| EvalError_value_error(format!("dict has no key '{attr}'")));
    }
    // A user-class instance exposes its stored fields (`{o.name}` /
    // `{0.value}`), the common `str.format` attribute-access pattern.
    if let Value::Instance(inst) = value {
        if let Some(v) = inst.fields.lock().get(attr) {
            return Ok(v.clone());
        }
    }
    // Type-slot attributes — `{x.real}`, `{x.imag}`, `{x.numerator}`, etc. —
    // resolve through the shared state-free getattr dispatch.
    if let Some(resolved) = crate::types::dispatch_getattr_opt(value, attr)? {
        return Ok(resolved);
    }
    Err(InterpreterError::AttributeError(format!(
        "'{}' object has no attribute '{attr}'",
        value.type_name()
    ))
    .into())
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
                    .lock()
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
            .lock()
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

#[allow(non_snake_case, reason = "matches the sibling EvalError_value_error constructor name")]
fn EvalError_index_error(message: String) -> EvalError {
    EvalError::Exception(ExceptionValue::new("IndexError", message))
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
/// `bytes % args` (and `bytearray % args`) — printf-style bytes formatting.
///
/// Implemented by decoding the template as latin-1 (lossless, byte↔codepoint),
/// converting bytes-like arguments to latin-1 strings so `%s`/`%b` insert their
/// raw bytes, reusing [`str_percent_format`], then re-encoding the result as
/// latin-1. Numeric/text conversions and flags/width/precision are identical to
/// str formatting. Known minor divergences: `%r`/`%a` render the object's *str*
/// repr rather than the bytes repr, and a str argument to `%s` is accepted
/// rather than rejected — both rare in real bytes formatting.
pub fn bytes_percent_format(template: &[u8], arg: &Value) -> EvalResult {
    let tmpl = decode_and_normalize_bytes_template(template);
    let converted = latin1_bytes_args(arg);
    let Value::String(result) = str_percent_format(&tmpl, &converted)? else {
        return Err(InterpreterError::Runtime("bytes format produced non-string".into()).into());
    };
    // latin-1 encode: every char is U+0000..=U+00FF from the decode/format above.
    Ok(Value::Bytes(result.chars().map(|c| c as u8).collect()))
}

/// Async `bytes % args` for when an operand is a user-class instance: identical
/// to [`bytes_percent_format`] but routes through [`str_percent_format_async`]
/// so instance operands are coerced via their numeric/text dunders.
pub async fn bytes_percent_format_async(
    state: &mut InterpreterState,
    template: &[u8],
    arg: &Value,
    tools: &Tools,
) -> EvalResult {
    let tmpl = decode_and_normalize_bytes_template(template);
    let converted = latin1_bytes_args(arg);
    let Value::String(result) = str_percent_format_async(state, &tmpl, &converted, tools).await?
    else {
        return Err(InterpreterError::Runtime("bytes format produced non-string".into()).into());
    };
    Ok(Value::Bytes(result.chars().map(|c| c as u8).collect()))
}

/// Decode a bytes format template as latin-1 and rewrite each `%b` conversion to
/// `%s` (equivalent for bytes formatting) so the shared str formatter accepts it.
fn decode_and_normalize_bytes_template(template: &[u8]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut i = 0;
    while i < template.len() {
        let c = template[i] as char;
        i += 1;
        out.push(c);
        if c != '%' {
            continue;
        }
        // `%%` — a literal percent, no conversion follows.
        if template.get(i) == Some(&b'%') {
            out.push('%');
            i += 1;
            continue;
        }
        // Optional `(mapping key)` — copy verbatim (its letters are not a conv).
        if template.get(i) == Some(&b'(') {
            while i < template.len() {
                let ch = template[i] as char;
                out.push(ch);
                i += 1;
                if ch == ')' {
                    break;
                }
            }
        }
        // Copy flags/width/.precision/length-mods up to the conversion letter.
        while i < template.len() {
            let ch = template[i] as char;
            i += 1;
            if ch.is_ascii_alphabetic() && !matches!(ch, 'l' | 'h' | 'L') {
                out.push(if ch == 'b' { 's' } else { ch });
                break;
            }
            out.push(ch);
        }
    }
    out
}

/// Convert bytes-like positional arguments to latin-1 strings so `%s`/`%b`
/// insert their raw bytes through the shared str formatter.
fn latin1_bytes_args(arg: &Value) -> Value {
    fn conv(v: &Value) -> Value {
        match v {
            Value::Bytes(b) => {
                Value::String(b.iter().map(|&x| x as char).collect::<String>().into())
            }
            Value::ByteArray(b) => {
                Value::String(b.lock().iter().map(|&x| x as char).collect::<String>().into())
            }
            other => other.clone(),
        }
    }
    match arg {
        Value::Tuple(items) => Value::Tuple(items.iter().map(conv).collect()),
        // A mapping's bytes values are left as-is (a rare corner).
        Value::Dict(_) => arg.clone(),
        other => conv(other),
    }
}

/// Consume the next positional argument as the integer value of a `*` width or
/// precision. Errors if it is missing or not an int, matching CPython.
fn percent_star_arg(positional: &[Value], next_arg: &mut usize) -> Result<i64, EvalError> {
    let v = positional.get(*next_arg).ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(
            "not enough arguments for format string".into(),
        ))
    })?;
    *next_arg += 1;
    match v {
        Value::Int(n) => Ok(*n),
        Value::Bool(b) => Ok(i64::from(*b)),
        other => {
            Err(InterpreterError::TypeError(format!("* wants int, not {}", other.type_name()))
                .into())
        }
    }
}

/// One rendered segment of a `%`-template: a literal run, or a conversion
/// paired with the (already arg-consumed) operand it formats. Splitting the
/// walk from the render lets the sync and async formatters share one source of
/// truth for argument consumption (including `*` width/precision args).
enum PercentPiece {
    Literal(String),
    Conv { spec: PercentSpec, value: Value },
}

/// Walk a `%`-template, parsing each conversion's spec and resolving the
/// operand it consumes (positional or `(name)` mapping). The `*` width and
/// precision arguments are consumed here too, so this is the single point that
/// advances the positional cursor. The trailing "not all arguments converted"
/// check runs at the end.
fn parse_percent_pieces(template: &str, arg: &Value) -> Result<Vec<PercentPiece>, EvalError> {
    let chars: Vec<char> = template.chars().collect();
    let positional: Vec<Value> = match arg {
        Value::Tuple(items) => items.clone(),
        Value::Dict(_) => Vec::new(),
        other => vec![other.clone()],
    };
    // Snapshot the mapping once so the per-field lookups below don't
    // each re-lock, and the shape stays `Option<IndexMap>`.
    let mapping = arg.as_dict().map(|m| m.lock().clone());

    let mut pieces: Vec<PercentPiece> = Vec::new();
    let mut lit = String::new();
    let mut next_arg = 0usize;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' {
            lit.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1; // consume '%'
        if chars.get(i) == Some(&'%') {
            lit.push('%');
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

        // Width — digits, or `*` to take the width from the next argument (a
        // negative `*` width means left-justify with the absolute width).
        let mut width = String::new();
        if chars.get(i) == Some(&'*') {
            i += 1;
            let w = percent_star_arg(&positional, &mut next_arg)?;
            if w < 0 {
                flag_minus = true;
                width = w.checked_neg().unwrap_or(i64::MAX).to_string();
            } else {
                width = w.to_string();
            }
        } else {
            while let Some(&c) = chars.get(i) {
                if c.is_ascii_digit() {
                    width.push(c);
                    i += 1;
                } else {
                    break;
                }
            }
        }

        // Precision — `.digits`, or `.*` to take it from the next argument (a
        // negative `*` precision is treated as if omitted, matching CPython).
        let precision: Option<String> = if chars.get(i) == Some(&'.') {
            i += 1;
            if chars.get(i) == Some(&'*') {
                i += 1;
                let p = percent_star_arg(&positional, &mut next_arg)?;
                if p < 0 { None } else { Some(p.to_string()) }
            } else {
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
            }
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
            let map = mapping.as_ref().ok_or_else(|| {
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
        if !lit.is_empty() {
            pieces.push(PercentPiece::Literal(std::mem::take(&mut lit)));
        }
        pieces.push(PercentPiece::Conv { spec, value });
    }
    if !lit.is_empty() {
        pieces.push(PercentPiece::Literal(lit));
    }

    // Positional over-supply is a TypeError in CPython ("not all arguments
    // converted"). Mapping form does not consume positionally, so skip then.
    if mapping_key_unused(mapping.as_ref(), next_arg, positional.len()) {
        return Err(InterpreterError::TypeError(
            "not all arguments converted during string formatting".into(),
        )
        .into());
    }

    Ok(pieces)
}

/// Printf-style `%`-formatting for builtin operands (`"%d" % 5`). Instance
/// operands with numeric/text dunders are handled by the async variant
/// [`str_percent_format_async`]; on this path they fall through to
/// [`format_percent_conversion`]'s type error, matching a plain builtin.
pub fn str_percent_format(template: &str, arg: &Value) -> EvalResult {
    let pieces = parse_percent_pieces(template, arg)?;
    let mut out = String::new();
    for piece in &pieces {
        match piece {
            PercentPiece::Literal(s) => out.push_str(s),
            PercentPiece::Conv { spec, value } => {
                out.push_str(&value_text(format_percent_conversion(value, spec)?));
            }
        }
    }
    Ok(Value::String(out.into()))
}

/// Printf-style `%`-formatting where an operand may be a user-class instance:
/// each conversion coerces its instance operand through the dunder CPython uses
/// (`%d`→`__index__`/`__int__`, `%x`/`%o`/`%c`→`__index__`, `%f`/`%e`/`%g`→
/// `__float__`, `%s`→`__str__`, `%r`/`%a`→`__repr__`) before rendering. Builtin
/// operands render exactly as on the sync path.
pub async fn str_percent_format_async(
    state: &mut InterpreterState,
    template: &str,
    arg: &Value,
    tools: &Tools,
) -> EvalResult {
    let pieces = parse_percent_pieces(template, arg)?;
    let mut out = String::new();
    for piece in &pieces {
        match piece {
            PercentPiece::Literal(s) => out.push_str(s),
            PercentPiece::Conv { spec, value } => {
                let coerced = coerce_percent_operand(state, value, spec.conv, tools).await?;
                // For an instance under `%s`/`%r`/`%a` the coercion already
                // produced the final str()/repr()/ascii() text, so format it as
                // plain text — otherwise `%r` would re-quote the rendered string.
                let eff = if matches!(value, Value::Instance(_))
                    && matches!(spec.conv, 's' | 'r' | 'a')
                {
                    PercentSpec { conv: 's', ..*spec }
                } else {
                    *spec
                };
                out.push_str(&value_text(format_percent_conversion(&coerced, &eff)?));
            }
        }
    }
    Ok(Value::String(out.into()))
}

/// Coerce a single `%`-conversion operand when it is a user-class instance,
/// dispatching the dunder CPython's C-level formatter would use for that
/// conversion. A non-instance operand — or an instance lacking the relevant
/// dunder — is returned unchanged so [`format_percent_conversion`] applies the
/// builtin path (and its canonical TypeError for the missing-dunder case).
async fn coerce_percent_operand(
    state: &mut InterpreterState,
    value: &Value,
    conv: char,
    tools: &Tools,
) -> Result<Value, EvalError> {
    if !matches!(value, Value::Instance(_)) {
        return Ok(value.clone());
    }
    match conv {
        // `%s` uses str(), `%r`/`%a` use repr()/ascii().
        's' => Ok(Value::String(
            crate::eval::render::render(
                state,
                value,
                crate::eval::render::RenderMode::Display,
                tools,
            )
            .await?
            .into(),
        )),
        'r' | 'a' => {
            let mode = if conv == 'a' {
                crate::eval::render::RenderMode::Ascii
            } else {
                crate::eval::render::RenderMode::Repr
            };
            Ok(Value::String(crate::eval::render::render(state, value, mode, tools).await?.into()))
        }
        // `%d`/`%i`/`%u` accept `__index__` or `__int__`; `%x`/`%X`/`%o`/`%c`
        // require `__index__` specifically (matching CPython's PyNumber_Index).
        'd' | 'i' | 'u' => {
            coerce_via_int_dunders(state, value, &["__index__", "__int__"], tools).await
        }
        'x' | 'X' | 'o' | 'c' => coerce_via_int_dunders(state, value, &["__index__"], tools).await,
        // Float conversions use `__float__`, falling back to `__index__`.
        'e' | 'E' | 'f' | 'F' | 'g' | 'G' => {
            for slot in ["__float__", "__index__"] {
                if let Some(res) =
                    crate::eval::op::instance_unary_dunder(state, value, slot, tools).await
                {
                    let r = res?;
                    return match r {
                        Value::Float(_) | Value::Int(_) | Value::BigInt(_) | Value::Bool(_) => {
                            Ok(r)
                        }
                        other => Err(InterpreterError::TypeError(format!(
                            "{slot} returned non-float (type {})",
                            other.type_name()
                        ))
                        .into()),
                    };
                }
            }
            Ok(value.clone())
        }
        _ => Ok(value.clone()),
    }
}

/// Resolve an instance to the integer one of `slots` (`__index__`/`__int__`)
/// returns, in order. Returns the original value unchanged when the instance
/// defines none of them, so the caller's builtin path raises the canonical
/// "a number is required" error.
async fn coerce_via_int_dunders(
    state: &mut InterpreterState,
    value: &Value,
    slots: &[&str],
    tools: &Tools,
) -> Result<Value, EvalError> {
    for slot in slots {
        if let Some(res) = crate::eval::op::instance_unary_dunder(state, value, slot, tools).await {
            let r = res?;
            return match r {
                Value::Int(_) | Value::BigInt(_) | Value::Bool(_) => Ok(r),
                other => Err(InterpreterError::TypeError(format!(
                    "{slot} returned non-int (type {})",
                    other.type_name()
                ))
                .into()),
            };
        }
    }
    Ok(value.clone())
}

/// Parsed `%`-conversion specifier.
#[derive(Clone, Copy)]
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
                // CPython splits the wording: the decimal conversions
                // (`d`/`i`/`u`) accept any real number, the radix conversions
                // (`o`/`x`/`X`) require an integer.
                let requirement = match spec.conv {
                    'o' | 'x' | 'X' => "an integer",
                    _ => "a real number",
                };
                return Err(InterpreterError::TypeError(format!(
                    "%{} format: {requirement} is required, not {}",
                    spec.conv,
                    value.type_name()
                ))
                .into());
            }
        },
        'e' | 'E' | 'f' | 'F' | 'g' | 'G' => Value::Float(value.as_float().ok_or_else(|| {
            // The float conversions carry no `%x format:` prefix in CPython.
            EvalError::from(InterpreterError::TypeError(format!(
                "must be real number, not {}",
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
