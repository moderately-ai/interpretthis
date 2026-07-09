// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Generator iterator methods on `Value::Lazy` (eager-yield buffers).
//!
//! The runtime materialises generator bodies into a buffered
//! [`Value::Lazy`] with a shared cursor in [`InterpreterState::lazy_cursors`].
//! That is enough for `for` / `list` / `next` consumers. Full coroutine
//! semantics (`send` injecting values into a suspended frame) need a
//! real stack machine; here we implement the CPython method *surface*
//! with eager-buffer semantics:
//!
//! - `next(g)` / `g.__next__()` — advance the cursor
//! - `g.send(None)` — same as next (first call must be None)
//! - `g.send(x)` after start — advances the cursor; `x` is discarded
//!   (documented divergence vs CPython resume-with-value)
//! - `g.throw(Exc[, val])` — raises into the caller (as if the
//!   generator re-raised); marks the generator exhausted
//! - `g.close()` — exhaust the cursor; return None

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    value::{ExceptionValue, Value},
};

/// True when `method` is a generator-iterator protocol name.
#[must_use]
pub(crate) fn is_generator_method(method: &str) -> bool {
    matches!(method, "send" | "throw" | "close" | "__next__")
}

/// Dispatch a generator method on a `Value::Lazy` receiver.
pub(crate) fn dispatch_generator_method(
    state: &mut InterpreterState,
    receiver: &Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> EvalResult {
    if let Some((name, _)) = kwargs.first() {
        return Err(InterpreterError::TypeError(format!(
            "{method}() got an unexpected keyword argument '{name}'"
        ))
        .into());
    }
    let Value::Lazy { items, cursor_id } = receiver else {
        return Err(InterpreterError::TypeError(format!(
            "'{}' object has no attribute '{method}'",
            receiver.type_name()
        ))
        .into());
    };
    let cursor = state.lazy_cursors.get(cursor_id).copied().unwrap_or(0);

    match method {
        "__next__" | "send" => {
            // send(value): first resume must be None (CPython).
            if method == "send" {
                let value = args.first().cloned().unwrap_or(Value::None);
                if cursor == 0 && !matches!(value, Value::None) {
                    return Err(InterpreterError::TypeError(
                        "can't send non-None value to a just-started generator".into(),
                    )
                    .into());
                }
                // Non-None send after start: discard value (eager buffer).
            } else if !args.is_empty() {
                return Err(
                    InterpreterError::TypeError("__next__() takes no arguments".into()).into()
                );
            }
            if cursor < items.len() {
                state.lazy_cursors.insert(*cursor_id, cursor + 1);
                Ok(items[cursor].clone())
            } else {
                Err(EvalError::Exception(ExceptionValue::new("StopIteration", String::new())))
            }
        }
        "close" => {
            if !args.is_empty() {
                return Err(InterpreterError::TypeError("close() takes no arguments".into()).into());
            }
            state.lazy_cursors.insert(*cursor_id, items.len());
            Ok(Value::None)
        }
        "throw" => {
            // Mark exhausted, then raise the requested exception at the
            // throw() call site (eager generators have no suspended frame
            // to inject into).
            state.lazy_cursors.insert(*cursor_id, items.len());
            let exc = throw_exception(args)?;
            Err(EvalError::Exception(exc))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'generator' object has no attribute '{method}'"
        ))
        .into()),
    }
}

fn throw_exception(args: &[Value]) -> Result<ExceptionValue, EvalError> {
    let Some(typ) = args.first() else {
        return Err(InterpreterError::TypeError("throw() takes at least 1 argument".into()).into());
    };
    match typ {
        Value::Exception(e) => Ok(e.clone()),
        Value::ExceptionType(name) => {
            let message = args.get(1).map(|v| format!("{v}")).unwrap_or_default();
            Ok(ExceptionValue::new(name.clone(), message))
        }
        Value::Class(name) => {
            let message = args.get(1).map(|v| format!("{v}")).unwrap_or_default();
            Ok(ExceptionValue::new(name.clone(), message))
        }
        other => Err(InterpreterError::TypeError(format!(
            "exceptions must derive from BaseException, not '{}'",
            other.type_name()
        ))
        .into()),
    }
}
