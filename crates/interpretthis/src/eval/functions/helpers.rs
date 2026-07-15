// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    tools::Tools,
    value::{ExceptionValue, Value},
};

/// Parse a Python `complex()` string literal — `"1+2j"`, `"3j"`, `"-1.5e3-2j"`,
/// `"(1+2j)"`, `"inf"`, `"nanj"`. Returns `None` on a malformed string (caller
/// raises `ValueError`). The imaginary part is marked by a trailing `j`/`J`; the
/// real/imag split is the last sign that is not the leading char and not part of
/// a float exponent (`e`/`E`).
pub(super) fn parse_complex_str(s: &str) -> Option<num_complex::Complex64> {
    use num_complex::Complex64;
    let mut t = s.trim();
    // Strip a single layer of surrounding parentheses.
    if let Some(inner) = t.strip_prefix('(') {
        t = inner.strip_suffix(')')?.trim();
    }
    if t.is_empty() {
        return None;
    }
    // No `j` suffix -> a purely real value.
    let Some(core) = t.strip_suffix(['j', 'J']) else {
        return t.parse::<f64>().ok().map(|re| Complex64::new(re, 0.0));
    };
    // Find the last '+'/'-' that separates real from imaginary (skip index 0 and
    // signs that follow an exponent marker).
    let mut split = None;
    let bytes = core.as_bytes();
    for (i, ch) in core.char_indices() {
        if i > 0 && (ch == '+' || ch == '-') {
            let prev = bytes[i - 1];
            if prev != b'e' && prev != b'E' {
                split = Some(i);
            }
        }
    }
    let coeff = |part: &str| -> Option<f64> {
        match part {
            "" | "+" => Some(1.0),
            "-" => Some(-1.0),
            other => other.parse::<f64>().ok(),
        }
    };
    match split {
        Some(k) => {
            let re = core[..k].parse::<f64>().ok()?;
            let im = coeff(&core[k..])?;
            Some(Complex64::new(re, im))
        }
        None => Some(Complex64::new(0.0, coeff(core)?)),
    }
}

/// `bytes.fromhex(hex_str)` — parse a hex string into bytes. CPython
/// allows ASCII whitespace between hex pairs and is case-insensitive.
pub(super) fn bytes_fromhex(args: &[Value]) -> EvalResult {
    let Some(Value::String(s)) = args.first() else {
        return Err(InterpreterError::TypeError("fromhex() requires a str argument".into()).into());
    };
    let cleaned: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if cleaned.len() % 2 != 0 {
        return Err(InterpreterError::ValueError(
            "non-hexadecimal number found in fromhex() arg".into(),
        )
        .into());
    }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let bytes = cleaned.as_bytes();
    for pair in bytes.chunks_exact(2) {
        let hi = hex_digit(pair[0])?;
        let lo = hex_digit(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(Value::Bytes(out))
}

/// The `byteorder` argument (positional or keyword), returning whether the
/// order is little-endian. Defaults to `"big"` (CPython 3.11+).
fn parse_byteorder(
    positional: Option<&Value>,
    kwargs: &IndexMap<String, Value>,
) -> Result<bool, EvalError> {
    match positional.or_else(|| kwargs.get("byteorder")) {
        None => Ok(false),
        Some(Value::String(s)) => match s.as_str() {
            "big" => Ok(false),
            "little" => Ok(true),
            _ => Err(InterpreterError::ValueError(
                "byteorder must be either 'little' or 'big'".into(),
            )
            .into()),
        },
        Some(other) => Err(InterpreterError::TypeError(format!(
            "byteorder must be str, not {}",
            other.type_name()
        ))
        .into()),
    }
}

/// `int.from_bytes(bytes, byteorder='big', *, signed=False)` — decode an
/// integer from its byte representation. Accepts a `bytes`/`bytearray` value or
/// any iterable of ints in `range(0, 256)`; the result promotes past `i64`.
pub(super) fn int_from_bytes(args: &[Value], kwargs: &IndexMap<String, Value>) -> EvalResult {
    use num_bigint::{BigInt, Sign};
    let Some(src) = args.first() else {
        return Err(InterpreterError::TypeError(
            "from_bytes() missing required argument 'bytes' (pos 1)".into(),
        )
        .into());
    };
    let byte_of = |v: &Value| -> Result<u8, EvalError> {
        match v {
            Value::Int(n) if (0..=255).contains(n) => Ok(*n as u8),
            Value::Bool(b) => Ok(u8::from(*b)),
            Value::Int(_) => {
                Err(InterpreterError::ValueError("bytes must be in range(0, 256)".into()).into())
            }
            other => Err(InterpreterError::TypeError(format!(
                "'{}' object cannot be interpreted as an integer",
                other.type_name()
            ))
            .into()),
        }
    };
    let mut be: Vec<u8> = match src {
        Value::Bytes(b) => b.clone(),
        Value::List(l) => l.lock().iter().map(&byte_of).collect::<Result<_, _>>()?,
        Value::Tuple(t) => t.iter().map(&byte_of).collect::<Result<_, _>>()?,
        other => {
            return Err(InterpreterError::TypeError(format!(
                "cannot convert '{}' object to bytes",
                other.type_name()
            ))
            .into());
        }
    };
    if parse_byteorder(args.get(1), kwargs)? {
        be.reverse();
    }
    let signed = kwargs.get("signed").is_some_and(Value::is_truthy);
    let mut n = BigInt::from_bytes_be(Sign::Plus, &be);
    if signed && be.first().is_some_and(|b| b & 0x80 != 0) {
        // High bit set under signed interpretation → two's-complement negative.
        n -= BigInt::from(1) << (8 * be.len());
    }
    Ok(crate::value::int_from_bigint(n))
}

/// `int.to_bytes(length=1, byteorder='big', *, signed=False)` — encode `value`
/// into exactly `length` bytes, raising `OverflowError` when it does not fit
/// (or is negative without `signed=True`), matching CPython.
pub(super) fn int_to_bytes(
    value: &num_bigint::BigInt,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> EvalResult {
    use num_bigint::BigInt;
    use num_traits::{Signed as _, Zero as _};
    let length: usize = match args.first().or_else(|| kwargs.get("length")) {
        None => 1,
        Some(Value::Int(n)) if *n >= 0 => usize::try_from(*n).unwrap_or(usize::MAX),
        Some(Value::Int(_)) => {
            return Err(InterpreterError::ValueError(
                "length argument must be non-negative".into(),
            )
            .into());
        }
        Some(other) => {
            return Err(InterpreterError::TypeError(format!(
                "'{}' object cannot be interpreted as an integer",
                other.type_name()
            ))
            .into());
        }
    };
    let little = parse_byteorder(args.get(1), kwargs)?;
    let signed = kwargs.get("signed").is_some_and(Value::is_truthy);
    if value.is_negative() && !signed {
        return Err(EvalError::Exception(crate::value::ExceptionValue::new(
            "OverflowError",
            "can't convert negative int to unsigned",
        )));
    }
    let overflow = || {
        EvalError::Exception(crate::value::ExceptionValue::new(
            "OverflowError",
            "int too big to convert",
        ))
    };
    // Zero bytes can only encode zero; avoids the `8*length - 1` underflow.
    if length == 0 {
        if value.is_zero() {
            return Ok(Value::Bytes(Vec::new()));
        }
        return Err(overflow());
    }
    let bits = 8 * length;
    let modulus = BigInt::from(1) << bits;
    // Representable window: signed is [-2^(bits-1), 2^(bits-1)), unsigned is
    // [0, 2^bits).
    let (lo, hi) = if signed {
        let half = BigInt::from(1) << (bits - 1);
        (-half.clone(), half)
    } else {
        (BigInt::from(0), modulus.clone())
    };
    if value < &lo || value >= &hi {
        return Err(overflow());
    }
    let image = if value.is_negative() { &modulus + value } else { value.clone() };
    let (_, mag) = image.to_bytes_be();
    // Left-pad (big-endian) to `length`; `mag` is already <= length bytes.
    let mut out = vec![0u8; length.saturating_sub(mag.len())];
    out.extend_from_slice(&mag);
    if little {
        out.reverse();
    }
    Ok(Value::Bytes(out))
}

/// `float.fromhex(s)` — parse the C99 hexadecimal-float form
/// (`0x1.8p+0`, sign/`inf`/`nan` accepted). The mantissa digits are
/// always hexadecimal; the optional `p` exponent is a decimal power of
/// two.
pub(super) fn float_fromhex(args: &[Value]) -> EvalResult {
    let s = match args {
        [Value::String(s)] => s.as_str(),
        [other] => {
            return Err(InterpreterError::TypeError(format!(
                "float.fromhex() argument must be str, not {}",
                other.type_name()
            ))
            .into());
        }
        _ => {
            return Err(
                InterpreterError::TypeError("fromhex() takes exactly one argument".into()).into()
            );
        }
    };
    parse_hex_float(s.trim()).map(Value::Float).ok_or_else(|| {
        EvalError::from(InterpreterError::ValueError(
            "invalid hexadecimal floating-point string".into(),
        ))
    })
}

/// Parse a C99 hex-float / special-value string into an `f64`.
fn parse_hex_float(input: &str) -> Option<f64> {
    let lower = input.to_ascii_lowercase();
    let (sign, body) = match lower.strip_prefix('-') {
        Some(rest) => (-1.0_f64, rest),
        None => (1.0_f64, lower.strip_prefix('+').unwrap_or(&lower)),
    };
    match body {
        "inf" | "infinity" => return Some(sign * f64::INFINITY),
        "nan" => return Some(f64::NAN),
        "" => return None,
        _ => {}
    }
    let body = body.strip_prefix("0x").unwrap_or(body);
    // Split off the binary exponent (`p<decimal>`), if any.
    let (mantissa, exp) = match body.split_once('p') {
        Some((m, e)) => (m, e.parse::<i32>().ok()?),
        None => (body, 0),
    };
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    let mut value = 0.0_f64;
    for c in int_part.chars() {
        value = value * 16.0 + f64::from(c.to_digit(16)?);
    }
    let mut scale = 1.0_f64 / 16.0;
    for c in frac_part.chars() {
        value += f64::from(c.to_digit(16)?) * scale;
        scale /= 16.0;
    }
    Some(sign * value * 2.0_f64.powi(exp))
}

/// `str.maketrans(...)` — build a translation table (a dict keyed by code
/// point) for `str.translate`. Three forms: a single dict (keys may be 1-char
/// strings or ints), two equal-length strings (positional char mapping), or a
/// third string whose characters map to `None` (deletion).
pub(super) fn str_maketrans(args: &[Value]) -> EvalResult {
    use crate::value::ValueKey;
    let cp = |c: char| ValueKey::Int(i64::from(u32::from(c)));
    let single_char_key = |s: &str| -> Result<ValueKey, EvalError> {
        let mut it = s.chars();
        match (it.next(), it.next()) {
            (Some(c), None) => Ok(cp(c)),
            _ => Err(InterpreterError::ValueError(
                "string keys in translate table must be of length 1".into(),
            )
            .into()),
        }
    };
    let mut map: IndexMap<ValueKey, Value> = IndexMap::new();
    match args {
        [Value::Dict(d)] => {
            for (k, v) in d.lock().iter() {
                let key = match k {
                    ValueKey::Int(_) => k.clone(),
                    ValueKey::String(s) => single_char_key(s)?,
                    _ => {
                        return Err(InterpreterError::TypeError(
                            "keys in translate table must be strings or integers".into(),
                        )
                        .into());
                    }
                };
                map.insert(key, v.clone());
            }
        }
        [Value::String(x), Value::String(y), rest @ ..] => {
            if x.chars().count() != y.chars().count() {
                return Err(InterpreterError::ValueError(
                    "the first two maketrans arguments must have equal length".into(),
                )
                .into());
            }
            for (cx, cy) in x.chars().zip(y.chars()) {
                map.insert(cp(cx), Value::Int(i64::from(u32::from(cy))));
            }
            match rest {
                [] => {}
                [Value::String(z)] => {
                    for cz in z.chars() {
                        map.insert(cp(cz), Value::None);
                    }
                }
                _ => {
                    return Err(InterpreterError::TypeError(
                        "maketrans third argument must be a str".into(),
                    )
                    .into());
                }
            }
        }
        _ => {
            return Err(InterpreterError::TypeError(
                "maketrans expects a dict, or two/three str arguments".into(),
            )
            .into());
        }
    }
    Ok(Value::Dict(crate::value::shared_dict(map)))
}

fn hex_digit(b: u8) -> Result<u8, EvalError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(InterpreterError::ValueError(format!(
            "non-hexadecimal character {:?} in fromhex() arg",
            b as char
        ))
        .into()),
    }
}

/// A stable object id for `id(x)`, consistent with `is`: `id(a) == id(b)` iff
/// `a is b`.
///
/// Reference types (list, instance, function, lambda, lru_cache) use their
/// shared `Arc`'s address — two distinct objects get distinct ids, an alias
/// shares one. Immutable value types have no stable address in the clone-on-load
/// model, so their id is derived from their `repr`, giving equal values equal
/// ids (matching the identity-as-equality choice `is` makes for immutables).
/// Regression: `id()` returned 0 for everything, so `id(a) == id(b)` was always
/// true.
pub(super) fn object_id(v: &Value) -> i64 {
    use std::sync::Arc;
    let raw: usize = match v {
        Value::List(a) => Arc::as_ptr(a).addr(),
        Value::Instance(i) => Arc::as_ptr(&i.fields).addr(),
        Value::Function(a) => Arc::as_ptr(a).addr(),
        Value::Lambda(a) => Arc::as_ptr(a).addr(),
        Value::LruCache(a) => Arc::as_ptr(a).addr(),
        other => {
            use std::hash::{Hash as _, Hasher as _};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            other.repr().hash(&mut hasher);
            hasher.finish() as usize
        }
    };
    // CPython ids are non-negative; clear the sign bit.
    (raw & (i64::MAX as usize)) as i64
}

/// Build a byte vector from an iterable of ints, for `bytes(iterable)`.
///
/// Each element must be an int in `range(0, 256)`; an out-of-range value raises
/// `ValueError`. Regression: the old inline versions (duplicated for `list` and
/// `tuple`) used `u8::try_from(n & 0xFF)`, and the mask made the conversion
/// always succeed — `bytes([300])` silently produced `b','` instead of raising.
pub(super) fn bytes_from_int_items(items: &[Value]) -> Result<Vec<u8>, EvalError> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let n = match item {
            Value::Int(i) => *i,
            Value::Bool(b) => i64::from(*b),
            _ => {
                return Err(InterpreterError::TypeError(
                    "bytes() argument items must be ints".into(),
                )
                .into());
            }
        };
        let byte = u8::try_from(n).map_err(|_| {
            EvalError::from(InterpreterError::ValueError("bytes must be in range(0, 256)".into()))
        })?;
        out.push(byte);
    }
    Ok(out)
}

/// `dict.fromkeys(iterable, value=None)` — build a dict mapping each
/// element of `iterable` to `value`. Async because `op::iter` may
/// dispatch a user-class `__iter__`/`__next__`.
pub(super) async fn dict_fromkeys(
    state: &mut InterpreterState,
    args: &[Value],
    tools: &Tools,
) -> EvalResult {
    let Some(iterable) = args.first() else {
        return Err(
            InterpreterError::TypeError("fromkeys() requires at least 1 argument".into()).into()
        );
    };
    let value = args.get(1).cloned().unwrap_or(Value::None);
    let items = crate::eval::op::iter(state, iterable, tools).await?;
    let mut map = IndexMap::new();
    for item in items {
        let key = crate::eval::op::key(state, &item, tools).await?;
        map.insert(key, value.clone());
    }
    Ok(Value::Dict(crate::value::shared_dict(map)))
}

/// "Not a list, can't `.sort()`" — shared between the place and
/// temporary arms of the `list.sort()` intercept in `eval_call`.
pub(super) fn list_sort_type_error(type_name: &str) -> EvalError {
    InterpreterError::AttributeError(format!("'{type_name}' object has no attribute 'sort'")).into()
}

/// Apply a key function (for min/max/sorted).
pub(super) async fn apply_key_fn(
    state: &mut InterpreterState,
    item: &Value,
    key_fn: Option<&Value>,
    tools: &Tools,
) -> EvalResult {
    match key_fn {
        Some(func) => {
            super::dispatch::call_value_as_function(
                state,
                func,
                std::slice::from_ref(item),
                &indexmap::IndexMap::new(),
                tools,
            )
            .await
        }
        None => Ok(item.clone()),
    }
}

/// Inputs to a decorate-sort-undecorate operation. Bundled so the
/// shared [`dsu_sort`] helper used by `sorted(...)` and `list.sort(...)`
/// stays under the workspace's 5-arg threshold (see `.claude/rules/rust.md`).
pub(crate) struct SortRequest<'a> {
    pub items: Vec<Value>,
    pub key_fn: Option<&'a Value>,
    pub reverse: bool,
}

/// Async-friendly sort via decorate-sort-undecorate: the key function
/// runs once per item up front, then a sync `sort_by` compares the
/// precomputed keys. Routes through [`apply_key_fn`] →
/// [`call_value_as_function`] so every callable shape works as
/// `key=` (BoundMethod, BuiltinTypeMethod, ModuleFunction, sentinel
/// strings). Used by the `sorted` builtin AND `list.sort()` method so
/// both surfaces share comparator + reversal semantics.
pub(crate) async fn dsu_sort(
    state: &mut InterpreterState,
    tools: &Tools,
    req: SortRequest<'_>,
) -> Result<Vec<Value>, EvalError> {
    let SortRequest { items, key_fn, reverse } = req;
    let mut decorated: Vec<(Value, Value)> = Vec::with_capacity(items.len());
    for item in items {
        let key = apply_key_fn(state, &item, key_fn, tools).await?;
        decorated.push((key, item));
    }
    // User-class instances need __lt__ dispatch, which is async. The
    // sync `sort_by` path can't reach the slot, so when any key is an
    // Instance we fall back to a stable insertion sort that awaits
    // each comparison. Pure-Rust types still get the sync sort_by
    // fast path — n log n with no allocations per compare.
    let any_instance = decorated.iter().any(|(k, _)| matches!(k, Value::Instance(_)));
    if any_instance {
        let mut sorted_dec: Vec<(Value, Value)> = Vec::with_capacity(decorated.len());
        for entry in decorated {
            let mut insert_at = sorted_dec.len();
            for (i, existing) in sorted_dec.iter().enumerate() {
                if crate::eval::op::lt(state, &entry.0, &existing.0, tools).await? {
                    insert_at = i;
                    break;
                }
            }
            sorted_dec.insert(insert_at, entry);
        }
        let mut sorted: Vec<Value> = sorted_dec.into_iter().map(|(_, v)| v).collect();
        if reverse {
            sorted.reverse();
        }
        return Ok(sorted);
    }
    // Capture the first comparison error out of the sort_by closure (which
    // cannot return a Result). Previously `compare_lt(...).unwrap_or(false)`
    // swallowed it, so `sorted([1, "a", 2])` returned a silently-wrong order
    // instead of raising `TypeError: '<' not supported between ...`.
    let mut cmp_err: Option<EvalError> = None;
    decorated.sort_by(|a, b| {
        use std::cmp::Ordering;
        if cmp_err.is_some() {
            return Ordering::Equal;
        }
        match crate::eval::operations::compare_lt(&a.0, &b.0) {
            Ok(true) => Ordering::Less,
            Ok(false) => match crate::eval::operations::compare_lt(&b.0, &a.0) {
                Ok(true) => Ordering::Greater,
                Ok(false) => Ordering::Equal,
                Err(e) => {
                    cmp_err = Some(e);
                    Ordering::Equal
                }
            },
            Err(e) => {
                cmp_err = Some(e);
                Ordering::Equal
            }
        }
    });
    if let Some(e) = cmp_err {
        return Err(e);
    }
    let mut sorted: Vec<Value> = decorated.into_iter().map(|(_, v)| v).collect();
    if reverse {
        sorted.reverse();
    }
    Ok(sorted)
}

/// Check if a value is an instance of a type by name. For user-class
/// instances, walks the class's MRO so `isinstance(child, Parent)`
/// returns True (Track B1 multi-level inheritance).
pub(super) fn check_isinstance(state: &InterpreterState, obj: &Value, type_name: &str) -> bool {
    // Everything is an `object`.
    if type_name == "object" {
        return true;
    }
    if let Value::Instance(inst) = obj {
        if inst.class_name == type_name {
            return true;
        }
        if let Some(class) = state.classes.get(&inst.class_name) {
            return class.mro.iter().any(|ancestor| ancestor == type_name);
        }
        return false;
    }
    obj.type_name() == type_name
        || match (obj, type_name) {
            // Python: bool is a subclass of int. Counter is a dict
            // subclass (Track B3); isinstance honours that even though
            // our Value::Counter is a distinct variant.
            (Value::Bool(_), "int") | (Value::Counter(_), "dict") => true,
            // Walk the builtin exception hierarchy, so
            // `isinstance(KeyError(), LookupError)` is True (was flat: it only
            // matched the exact type or "Exception").
            (Value::Exception(e), tn) => {
                crate::eval::exceptions::builtin_exception_issubclass(&e.type_name, tn)
            }
            _ => false,
        }
}

/// Extract the comparable type name from an `isinstance`/`issubclass` type
/// argument: a class/type object yields its name; a built-in/exception name
/// sentinel string is stripped of its prefix; a `ModuleFunction` (e.g.
/// `collections.Counter` accessed via attribute) yields its function name.
/// The built-in type names our interpreter recognises as classes. Used to
/// tell a type object (`bool`, `int`) apart from a built-in *function*
/// (`len`, `sorted`) — both surface as `Value::BuiltinName`, but only the
/// former is a valid `issubclass`/`type`-argument. Exception types carry
/// their own `Value::ExceptionType` sentinel and are handled separately.
pub(super) const BUILTIN_TYPE_NAMES: &[&str] = &[
    "int",
    "float",
    "complex",
    "bool",
    "str",
    "bytes",
    "bytearray",
    "list",
    "tuple",
    "dict",
    "set",
    "frozenset",
    "range",
    "type",
    "object",
    "slice",
    "memoryview",
    "NoneType",
];

/// Built-in subclass relationship for `issubclass` when the child is a
/// built-in (not user-defined) type: reflexive, everything is `object`,
/// `bool` ⊂ `int`, `Counter` ⊂ `dict`, and the exception hierarchy.
pub(super) fn builtin_type_issubclass(child: &str, target: &str) -> bool {
    if target == "object" || child == target {
        return true;
    }
    match (child, target) {
        ("bool", "int") | ("Counter", "dict") => true,
        _ => crate::eval::exceptions::builtin_exception_issubclass(child, target),
    }
}

pub(super) fn type_arg_name(value: &Value) -> String {
    match value {
        Value::Class(n) | Value::Type(n) | Value::BuiltinName(n) | Value::ExceptionType(n) => {
            n.clone()
        }
        Value::ModuleFunction { name, .. } => name.clone(),
        other => format!("{other}"),
    }
}

/// Parse a string into a Python `int` for `int(str, base)`.
///
/// Correct and panic-free, replacing a `from_str_radix` call whose base was
/// unvalidated (`int("10", 0)` and any base > 36 panicked). Handles:
/// - base validation: `0` or `2..=36`, else `ValueError`;
/// - an optional leading `+`/`-`;
/// - base-0 prefix auto-detection (`0x`/`0o`/`0b`), and the matching prefix for
///   base 2/8/16;
/// - single underscores between digits (CPython's readability separators);
/// - arbitrary precision — the result promotes to `BigInt`, so a long literal is
///   exact rather than overflowing `i64`.
pub(super) fn parse_int_str(raw: &str, base: i64) -> Result<Value, EvalError> {
    use num_traits::Num as _;

    let invalid = || {
        EvalError::Exception(ExceptionValue::new(
            "ValueError",
            format!("invalid literal for int() with base {base}: {raw:?}"),
        ))
    };

    if base != 0 && !(2..=36).contains(&base) {
        return Err(EvalError::Exception(ExceptionValue::new(
            "ValueError",
            "int() base must be >= 2 and <= 36, or 0",
        )));
    }

    let trimmed = raw.trim();
    let (negative, rest) = match trimmed.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };

    // Resolve the radix and strip any base prefix.
    let (radix, digits_raw): (u32, &str) = if base == 0 {
        strip_ci(rest, "0x").map_or_else(
            || {
                strip_ci(rest, "0o")
                    .map_or_else(|| strip_ci(rest, "0b").map_or((10, rest), |r| (2, r)), |r| (8, r))
            },
            |r| (16, r),
        )
    } else {
        let radix = base as u32;
        let stripped = match radix {
            16 => strip_ci(rest, "0x"),
            8 => strip_ci(rest, "0o"),
            2 => strip_ci(rest, "0b"),
            _ => None,
        };
        (radix, stripped.unwrap_or(rest))
    };

    let cleaned = clean_underscores(digits_raw).ok_or_else(invalid)?;
    if cleaned.is_empty() {
        return Err(invalid());
    }
    // `int("010", 0)` is rejected by CPython: with base 0 and no prefix a leading
    // zero is only allowed when the value is all zeros.
    if base == 0
        && radix == 10
        && cleaned.len() > 1
        && cleaned.starts_with('0')
        && cleaned.bytes().any(|b| b != b'0')
    {
        return Err(invalid());
    }

    let mut big = num_bigint::BigInt::from_str_radix(&cleaned, radix).map_err(|_| invalid())?;
    if negative {
        big = -big;
    }
    Ok(crate::value::int_from_bigint(big))
}

/// Case-insensitively strip a two-char ASCII prefix (`0x`/`0o`/`0b`).
fn strip_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let bytes = s.as_bytes();
    let pfx = prefix.as_bytes();
    if bytes.len() >= pfx.len() && bytes[..pfx.len()].eq_ignore_ascii_case(pfx) {
        Some(&s[pfx.len()..])
    } else {
        None
    }
}

/// Validate and strip CPython-style digit-group underscores. Returns `None` if
/// an underscore is leading, trailing, or doubled (all rejected by CPython).
fn clean_underscores(s: &str) -> Option<String> {
    if s.is_empty() {
        return Some(String::new());
    }
    let bytes = s.as_bytes();
    if bytes[0] == b'_' || bytes[bytes.len() - 1] == b'_' {
        return None;
    }
    let mut out = String::with_capacity(s.len());
    let mut prev_underscore = false;
    for &b in bytes {
        if b == b'_' {
            if prev_underscore {
                return None;
            }
            prev_underscore = true;
        } else {
            prev_underscore = false;
            out.push(b as char);
        }
    }
    Some(out)
}

/// `pow(base, exp, mod)` — integer modular exponentiation. Computes
/// `base ** exp % mod` efficiently via square-and-multiply, without
/// materializing the (potentially astronomical) intermediate value.
/// CPython requires integer operands for the 3-arg form; non-int operands
/// raise `TypeError`.
/// Modular inverse of `a` mod `m` via the extended Euclidean algorithm, or
/// `None` when `gcd(a, m) != 1` (no inverse exists). The result is reduced into
/// `[0, |m|)`.
fn mod_inverse(a: i64, m: i64) -> Option<i64> {
    let modulus = m.unsigned_abs() as i128;
    let (mut old_r, mut r) = (i128::from(a).rem_euclid(modulus), modulus);
    let (mut old_s, mut s) = (1_i128, 0_i128);
    while r != 0 {
        let q = old_r / r;
        (old_r, r) = (r, old_r - q * r);
        (old_s, s) = (s, old_s - q * s);
    }
    if old_r != 1 {
        return None; // not coprime
    }
    let inv = old_s.rem_euclid(modulus);
    i64::try_from(inv).ok()
}

pub(super) fn pow_three_arg(
    base: &Value,
    exp: &Value,
    modulus: &Value,
) -> Result<Value, EvalError> {
    let base_i = match base {
        Value::Int(b) => *b,
        Value::Bool(b) => i64::from(*b),
        _ => {
            return Err(InterpreterError::TypeError(
                "pow() 3rd argument not allowed unless all arguments are integers".into(),
            )
            .into());
        }
    };
    let exp_i = match exp {
        Value::Int(e) => *e,
        Value::Bool(e) => i64::from(*e),
        _ => {
            return Err(InterpreterError::TypeError(
                "pow() 3rd argument not allowed unless all arguments are integers".into(),
            )
            .into());
        }
    };
    let mod_i = match modulus {
        Value::Int(m) => *m,
        Value::Bool(m) => i64::from(*m),
        _ => {
            return Err(InterpreterError::TypeError(
                "pow() 3rd argument not allowed unless all arguments are integers".into(),
            )
            .into());
        }
    };
    if mod_i == 0 {
        return Err(EvalError::Exception(ExceptionValue::new(
            "ValueError",
            "pow() 3rd argument cannot be 0",
        )));
    }
    // A negative exponent computes the modular inverse of the base (CPython
    // 3.8+), raised to the exponent's magnitude. It exists only when the base
    // is coprime with the modulus.
    let (effective_base, exp_u) = if exp_i < 0 {
        let inv = mod_inverse(base_i.rem_euclid(mod_i), mod_i).ok_or_else(|| {
            EvalError::Exception(ExceptionValue::new(
                "ValueError",
                "base is not invertible for the given modulus",
            ))
        })?;
        (inv, exp_i.unsigned_abs())
    } else {
        (base_i, exp_i.unsigned_abs())
    };
    // Square-and-multiply: O(log exp) i64 multiplications. The `rem_euclid`
    // keeps the result in [0, mod_i) matching CPython's sign rule for modulo
    // (result sign follows divisor). Use u128 for the intermediate product to
    // avoid overflow on the squarings.
    let m = mod_i.unsigned_abs();
    let mut result: u128 = 1;
    let mut b: u128 = u128::from(effective_base.rem_euclid(mod_i).unsigned_abs());
    let mut e: u64 = exp_u;
    let mod_u: u128 = m.into();
    while e > 0 {
        if e & 1 == 1 {
            result = result * b % mod_u;
        }
        e >>= 1;
        b = b * b % mod_u;
    }
    // result is < mod_u, and mod_u fits in i64::MAX::abs as u128, so the
    // narrowing is exact.
    let signed = i64::try_from(result).map_err(|err| {
        EvalError::from(InterpreterError::Runtime(format!("pow() result out of i64 range: {err}")))
    })?;
    // CPython matches the modulus sign: `pow(2, 3, -5)` -> `-2` not `3`.
    if mod_i < 0 && signed != 0 { Ok(Value::Int(signed + mod_i)) } else { Ok(Value::Int(signed)) }
}
