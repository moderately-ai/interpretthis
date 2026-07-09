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
    value::Value,
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
                EvalError::from(InterpreterError::Runtime("pop from an empty deque".into()))
            })?;
            let freed = place::to_isize(estimate_value_size(&popped));
            Ok(MethodOutcome { value: popped, mem_delta: -freed })
        }
        "popleft" => {
            let popped = items.pop_front().ok_or_else(|| {
                EvalError::from(InterpreterError::Runtime("pop from an empty deque".into()))
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
        _ => Err(InterpreterError::AttributeError(format!(
            "'deque' object has no attribute '{method}'"
        ))
        .into()),
    }
}
