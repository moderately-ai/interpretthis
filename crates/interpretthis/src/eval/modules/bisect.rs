// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `bisect` module — binary search / ordered
//! insertion over a sorted list. `insort_*` mutate the list in place
//! (CPython reference semantics; the shared list handle is preserved by
//! `resolve_method_args`). Ordering uses the shared sync `<` comparator.

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{modules::need_arg, operations::compare_lt},
    state::InterpreterState,
    tools::Tools,
    value::Value,
};

pub struct BisectModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for BisectModule {
    fn name(&self) -> &'static str {
        "bisect"
    }
    fn has_function(&self, name: &str) -> bool {
        matches!(
            name,
            "bisect_left" | "bisect_right" | "bisect" | "insort_left" | "insort_right" | "insort"
        )
    }
    async fn call(
        &self,
        state: &mut InterpreterState,
        func: &str,
        args: &[Value],
        kwargs: &IndexMap<String, Value>,
        tools: &Tools,
    ) -> EvalResult {
        let right = matches!(func, "bisect_right" | "bisect" | "insort_right" | "insort");
        let insert = matches!(func, "insort_left" | "insort_right" | "insort");
        let Some(Value::List(items)) = args.first() else {
            return Err(InterpreterError::TypeError(format!(
                "{func}() argument must be a list, not '{}'",
                args.first().map_or("nothing", Value::type_name)
            ))
            .into());
        };
        let list = items.clone();
        let x = need_arg(func, args, 1)?.clone();
        let key_fn = kwargs.get("key").filter(|v| !matches!(v, Value::None)).cloned();

        let snapshot = list.lock().clone();
        let len = snapshot.len();
        let mut lo = opt_bound(args.get(2), 0)?;
        let mut hi = opt_bound(args.get(3), len)?.min(len);
        if lo > len {
            return Err(InterpreterError::ValueError("lo must be non-negative".into()).into());
        }
        // Binary search. `right` chooses the insertion side on ties.
        let empty = IndexMap::new();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let elem = match &key_fn {
                Some(f) => {
                    crate::eval::modules::call_callable(
                        state,
                        f,
                        std::slice::from_ref(&snapshot[mid]),
                        &empty,
                        tools,
                    )
                    .await?
                }
                None => snapshot[mid].clone(),
            };
            // left:  a[mid] < x  → search right half
            // right: x < a[mid]  → search left half (so equal keys land right)
            let go_right = if right { !compare_lt(&x, &elem)? } else { compare_lt(&elem, &x)? };
            if go_right {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if insert {
            list.lock().insert(lo, x);
            return Ok(Value::None);
        }
        Ok(Value::Int(i64::try_from(lo).unwrap_or(i64::MAX)))
    }
}

/// Resolve an optional `lo`/`hi` positional bound (int, defaulting to
/// `default`). Non-int raises TypeError.
fn opt_bound(arg: Option<&Value>, default: usize) -> Result<usize, EvalError> {
    match arg {
        None | Some(Value::None) => Ok(default),
        Some(Value::Int(n)) if *n >= 0 => Ok(usize::try_from(*n).unwrap_or(usize::MAX)),
        Some(Value::Int(_)) => {
            Err(InterpreterError::ValueError("lo must be non-negative".into()).into())
        }
        Some(other) => Err(InterpreterError::TypeError(format!(
            "'{}' object cannot be interpreted as an integer",
            other.type_name()
        ))
        .into()),
    }
}
