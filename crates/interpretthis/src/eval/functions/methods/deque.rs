// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `collections.deque` method dispatch — append/appendleft/pop/popleft,
//! extend/extendleft, rotate, clear, copy. Maxlen enforcement happens
//! on every push so bounded deques stay bounded.

use std::collections::VecDeque;

use super::super::{MethodOutcome, arg1};
use crate::{
    error::{EvalError, InterpreterError},
    eval::{control_flow::iterate_value, place},
    state::estimate_value_size,
    value::{ExceptionValue, Value},
};

/// Methods on `collections.deque`. The deque is mutated in place;
/// memory accounting deltas (signed bytes) propagate via the
/// MethodOutcome shape. Maxlen enforcement happens here on every
/// push so the bounded form stays bounded. All byte-count conversions
/// route through `place::to_isize` / `place::size_delta`, which
/// saturate at isize::MAX rather than wrapping (sizes are bounded by
/// the interpreter's memory limit, always well below isize::MAX).
pub(crate) fn dispatch_deque_method(
    items: &mut VecDeque<Value>,
    maxlen: Option<&usize>,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "append" => {
            let val = arg1(method, args)?.clone();
            let added = place::to_isize(estimate_value_size(&val));
            items.push_back(val);
            let mut delta = added;
            if let Some(cap) = maxlen {
                while items.len() > *cap {
                    if let Some(dropped) = items.pop_front() {
                        delta =
                            delta.saturating_sub(place::to_isize(estimate_value_size(&dropped)));
                    }
                }
            }
            Ok(MethodOutcome { value: Value::None, mem_delta: delta })
        }
        "appendleft" => {
            let val = arg1(method, args)?.clone();
            let added = place::to_isize(estimate_value_size(&val));
            items.push_front(val);
            let mut delta = added;
            if let Some(cap) = maxlen {
                while items.len() > *cap {
                    if let Some(dropped) = items.pop_back() {
                        delta =
                            delta.saturating_sub(place::to_isize(estimate_value_size(&dropped)));
                    }
                }
            }
            Ok(MethodOutcome { value: Value::None, mem_delta: delta })
        }
        "pop" => {
            let popped = items.pop_back().ok_or_else(|| {
                EvalError::Exception(ExceptionValue::new("IndexError", "pop from an empty deque"))
            })?;
            let freed = place::to_isize(estimate_value_size(&popped));
            Ok(MethodOutcome { value: popped, mem_delta: -freed })
        }
        "popleft" => {
            let popped = items.pop_front().ok_or_else(|| {
                EvalError::Exception(ExceptionValue::new("IndexError", "pop from an empty deque"))
            })?;
            let freed = place::to_isize(estimate_value_size(&popped));
            Ok(MethodOutcome { value: popped, mem_delta: -freed })
        }
        "extend" => {
            let iter = iterate_value(arg1(method, args)?)?;
            let mut delta = 0isize;
            for val in iter {
                delta = delta.saturating_add(place::to_isize(estimate_value_size(&val)));
                items.push_back(val);
                if let Some(cap) = maxlen {
                    while items.len() > *cap {
                        if let Some(dropped) = items.pop_front() {
                            delta = delta
                                .saturating_sub(place::to_isize(estimate_value_size(&dropped)));
                        }
                    }
                }
            }
            Ok(MethodOutcome { value: Value::None, mem_delta: delta })
        }
        "extendleft" => {
            // CPython's extendleft adds in REVERSE order — the last
            // item iterated ends up at the very front.
            let iter = iterate_value(arg1(method, args)?)?;
            let mut delta = 0isize;
            for val in iter {
                delta = delta.saturating_add(place::to_isize(estimate_value_size(&val)));
                items.push_front(val);
                if let Some(cap) = maxlen {
                    while items.len() > *cap {
                        if let Some(dropped) = items.pop_back() {
                            delta = delta
                                .saturating_sub(place::to_isize(estimate_value_size(&dropped)));
                        }
                    }
                }
            }
            Ok(MethodOutcome { value: Value::None, mem_delta: delta })
        }
        "rotate" => {
            // rotate(n) — positive n rotates right (move back-to-front).
            let n = match args.first() {
                None => 1i64,
                Some(Value::Int(n)) => *n,
                Some(Value::Bool(b)) => i64::from(*b),
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "rotate() expected an integer (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            if items.is_empty() {
                return Ok(MethodOutcome::pure(Value::None));
            }
            let len = i64::try_from(items.len()).map_err(|_| {
                EvalError::from(InterpreterError::Runtime("deque length overflows i64".into()))
            })?;
            let shift_signed = n.rem_euclid(len);
            let shift = usize::try_from(shift_signed).map_err(|_| {
                EvalError::from(InterpreterError::Runtime("rotate shift out of range".into()))
            })?;
            items.rotate_right(shift);
            Ok(MethodOutcome::pure(Value::None))
        }
        "clear" => {
            let freed: usize = items.iter().map(estimate_value_size).sum();
            items.clear();
            Ok(MethodOutcome::shrank(Value::None, freed))
        }
        "copy" => {
            let copied = Value::Deque { items: items.clone(), maxlen: maxlen.copied() };
            Ok(MethodOutcome::pure(copied))
        }
        // `deque.index(x)` — first index of `x`, else ValueError (as `list`).
        "index" => {
            let target = arg1(method, args)?;
            for (i, item) in items.iter().enumerate() {
                if crate::eval::operations::values_equal_pub(item, target) {
                    return Ok(MethodOutcome::pure(Value::Int(i as i64)));
                }
            }
            Err(InterpreterError::ValueError(format!("{} is not in deque", target.repr())).into())
        }
        // `deque.count(x)` — number of occurrences of `x`.
        "count" => {
            let target = arg1(method, args)?;
            let n = items
                .iter()
                .filter(|it| crate::eval::operations::values_equal_pub(it, target))
                .count();
            Ok(MethodOutcome::pure(Value::Int(n as i64)))
        }
        // `deque.insert(i, x)` — insert before position `i` (clamped, negative
        // aware). A bounded deque already at capacity raises IndexError.
        "insert" => {
            let raw = match args.first() {
                Some(Value::Int(i)) => *i,
                Some(Value::Bool(b)) => i64::from(*b),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "insert() argument 1 must be an integer".into(),
                    )
                    .into());
                }
            };
            let value = args.get(1).cloned().unwrap_or(Value::None);
            if maxlen.is_some_and(|&m| items.len() >= m) {
                return Err(EvalError::Exception(ExceptionValue::new(
                    "IndexError",
                    "deque already at its maximum size",
                )));
            }
            let len = items.len() as i64;
            let idx = raw.clamp(-len, len);
            let idx = if idx < 0 { (idx + len).max(0) } else { idx.min(len) } as usize;
            let grew = estimate_value_size(&value);
            items.insert(idx, value);
            Ok(MethodOutcome::grew(Value::None, grew))
        }
        // `deque.remove(x)` — remove first occurrence, else ValueError.
        "remove" => {
            let target = arg1(method, args)?;
            let pos =
                items.iter().position(|it| crate::eval::operations::values_equal_pub(it, target));
            match pos {
                Some(i) => {
                    let removed = items.remove(i);
                    let freed = removed.as_ref().map_or(0, estimate_value_size);
                    Ok(MethodOutcome::shrank(Value::None, freed))
                }
                None => {
                    Err(InterpreterError::ValueError("deque.remove(x): x not in deque".into())
                        .into())
                }
            }
        }
        // `deque.reverse()` — reverse in place.
        "reverse" => {
            items.make_contiguous().reverse();
            Ok(MethodOutcome::pure(Value::None))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'deque' object has no attribute '{method}'"
        ))
        .into()),
    }
}
