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
    Ok(Value::Dict(map))
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
    decorated.sort_by(|a, b| {
        if crate::eval::operations::compare_lt(&a.0, &b.0).unwrap_or(false) {
            std::cmp::Ordering::Less
        } else if crate::eval::operations::compare_lt(&b.0, &a.0).unwrap_or(false) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
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
            (Value::Exception(e), tn) => e.type_name == tn || tn == "Exception",
            _ => false,
        }
}

/// Extract the comparable type name from an `isinstance`/`issubclass` type
/// argument: a class/type object yields its name; a built-in/exception name
/// sentinel string is stripped of its prefix; a `ModuleFunction` (e.g.
/// `collections.Counter` accessed via attribute) yields its function name.
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
    if exp_i < 0 {
        // CPython allows this when gcd(base, mod) == 1 (modular inverse),
        // but the modinverse machinery is out of scope for A3. Raise the
        // CPython-shaped error for now.
        return Err(EvalError::Exception(ExceptionValue::new(
            "ValueError",
            "pow() 2nd argument cannot be negative when 3rd argument specified",
        )));
    }
    // Square-and-multiply: O(log exp_i) i64 multiplications. The
    // `rem_euclid` keeps the result in [0, mod_i) matching CPython's
    // sign rule for modulo (result sign follows divisor). Use u128 for
    // the intermediate product to avoid overflow on the squarings.
    let m = mod_i.unsigned_abs();
    let mut result: u128 = 1;
    let mut b: u128 = u128::from(base_i.rem_euclid(mod_i).unsigned_abs());
    // exp_i >= 0 was verified above, so the cast through u64 is sign-preserving.
    let exp_u = u64::try_from(exp_i).map_err(|err| {
        EvalError::from(InterpreterError::Runtime(format!("pow() exponent out of range: {err}")))
    })?;
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
