// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `heapq` module — a binary min-heap maintained
//! over an ordinary list. The sift routines mirror CPython's exact
//! `_siftup` / `_siftdown` so the in-place array arrangement (not just
//! the pop order) matches. Element ordering uses the shared sync `<`
//! comparator, so uncomparable elements raise `TypeError` as in CPython.

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{control_flow::iterate_value, modules::need_arg, operations::compare_lt},
    state::InterpreterState,
    tools::Tools,
    value::{Value, shared_list},
};

pub struct HeapqModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for HeapqModule {
    fn name(&self) -> &'static str {
        "heapq"
    }
    fn has_function(&self, name: &str) -> bool {
        matches!(
            name,
            "heapify"
                | "heappush"
                | "heappop"
                | "heappushpop"
                | "heapreplace"
                | "nsmallest"
                | "nlargest"
                | "merge"
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
        match func {
            "heapify" => with_heap(args, |heap| {
                let n = heap.len();
                for i in (0..n / 2).rev() {
                    siftup(heap, i)?;
                }
                Ok(Value::None)
            }),
            "heappush" => {
                let item = need_arg(func, args, 1)?.clone();
                with_heap(args, |heap| {
                    heap.push(item);
                    let last = heap.len() - 1;
                    siftdown(heap, 0, last)?;
                    Ok(Value::None)
                })
            }
            "heappop" => with_heap(args, pop_min),
            "heappushpop" => {
                // Push then pop the smallest; if the new item is <= the
                // current min it is returned directly (no heap change).
                let item = need_arg(func, args, 1)?.clone();
                with_heap(args, |heap| {
                    if !heap.is_empty() && compare_lt(&heap[0], &item)? {
                        let returned = std::mem::replace(&mut heap[0], item);
                        siftup(heap, 0)?;
                        return Ok(returned);
                    }
                    Ok(item)
                })
            }
            "heapreplace" => {
                // Pop the smallest and push the new item in one step.
                let item = need_arg(func, args, 1)?.clone();
                with_heap(args, |heap| {
                    if heap.is_empty() {
                        return Err(EvalError::Exception(crate::value::ExceptionValue::new(
                            "IndexError",
                            "index out of range",
                        )));
                    }
                    let returned = std::mem::replace(&mut heap[0], item);
                    siftup(heap, 0)?;
                    Ok(returned)
                })
            }
            "nsmallest" => n_extreme(state, func, args, kwargs, tools, false).await,
            "nlargest" => n_extreme(state, func, args, kwargs, tools, true).await,
            "merge" => {
                // merge(*iterables, key=None, reverse=False) — CPython
                // returns a lazy iterator; the eager model materialises
                // the merged, globally-sorted result.
                let reverse = kwargs.get("reverse").is_some_and(Value::is_truthy);
                let mut merged: Vec<Value> = Vec::new();
                for it in args {
                    merged.extend(iterate_value(it)?);
                }
                sort_values(&mut merged, reverse)?;
                Ok(Value::List(shared_list(merged)))
            }
            _ => Err(InterpreterError::AttributeError(format!(
                "module 'heapq' has no attribute '{func}'"
            ))
            .into()),
        }
    }
}

/// Run `f` against the first argument's backing list (mutating it in
/// place, as `heapq` does). Non-list first args raise the CPython-style
/// error.
fn with_heap(args: &[Value], f: impl FnOnce(&mut Vec<Value>) -> EvalResult) -> EvalResult {
    let Some(Value::List(items)) = args.first() else {
        return Err(InterpreterError::TypeError(format!(
            "heap argument must be a list, not '{}'",
            args.first().map_or("nothing", Value::type_name)
        ))
        .into());
    };
    let mut guard = items.lock();
    f(&mut guard)
}

/// Pop and return the smallest element, restoring the heap invariant.
fn pop_min(heap: &mut Vec<Value>) -> EvalResult {
    let Some(last) = heap.pop() else {
        return Err(EvalError::Exception(crate::value::ExceptionValue::new(
            "IndexError",
            "index out of range",
        )));
    };
    if heap.is_empty() {
        return Ok(last);
    }
    let returned = std::mem::replace(&mut heap[0], last);
    siftup(heap, 0)?;
    Ok(returned)
}

/// CPython's `_siftdown`: bubble `heap[pos]` up toward `startpos` while
/// it is smaller than its parent.
fn siftdown(heap: &mut [Value], startpos: usize, pos: usize) -> Result<(), EvalError> {
    let mut pos = pos;
    let newitem = heap[pos].clone();
    while pos > startpos {
        let parentpos = (pos - 1) >> 1;
        if compare_lt(&newitem, &heap[parentpos])? {
            heap[pos] = heap[parentpos].clone();
            pos = parentpos;
            continue;
        }
        break;
    }
    heap[pos] = newitem;
    Ok(())
}

/// CPython's `_siftup`: move the smaller child up until the leaf, then
/// sift the original item down into place.
fn siftup(heap: &mut [Value], pos: usize) -> Result<(), EvalError> {
    let endpos = heap.len();
    let startpos = pos;
    let mut pos = pos;
    let newitem = heap[pos].clone();
    let mut childpos = 2 * pos + 1;
    while childpos < endpos {
        let rightpos = childpos + 1;
        // Pick the smaller child (CPython: `not heap[child] < heap[right]`).
        if rightpos < endpos && !compare_lt(&heap[childpos], &heap[rightpos])? {
            childpos = rightpos;
        }
        heap[pos] = heap[childpos].clone();
        pos = childpos;
        childpos = 2 * pos + 1;
    }
    heap[pos] = newitem;
    siftdown(heap, startpos, pos)
}

/// `nsmallest` / `nlargest` — materialise, sort (honouring an optional
/// `key`), and take the first `n`.
async fn n_extreme(
    state: &mut InterpreterState,
    func: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
    largest: bool,
) -> EvalResult {
    let n = match need_arg(func, args, 0)? {
        Value::Int(v) => (*v).max(0),
        Value::Bool(b) => i64::from(*b),
        other => {
            return Err(InterpreterError::TypeError(format!(
                "'{}' object cannot be interpreted as an integer",
                other.type_name()
            ))
            .into());
        }
    };
    let items = iterate_value(need_arg(func, args, 1)?)?;
    let key_fn =
        args.get(2).or_else(|| kwargs.get("key")).filter(|v| !matches!(v, Value::None)).cloned();
    // Decorate with keys (calling the user key fn if present), sort by
    // key, then undecorate.
    let empty = IndexMap::new();
    let mut decorated: Vec<(Value, Value)> = Vec::with_capacity(items.len());
    for item in items {
        let key = match &key_fn {
            Some(f) => {
                crate::eval::modules::call_callable(
                    state,
                    f,
                    std::slice::from_ref(&item),
                    &empty,
                    tools,
                )
                .await?
            }
            None => item.clone(),
        };
        decorated.push((key, item));
    }
    // Sort by key — ascending for `nsmallest`, descending for `nlargest`.
    // The sort is stable and ties compare Equal, so equal-key elements keep
    // their original order in BOTH directions (CPython breaks key ties by
    // first-seen; a sort-then-reverse would invert nlargest's ties).
    let mut err: Option<EvalError> = None;
    decorated.sort_by(|a, b| {
        if err.is_some() {
            return std::cmp::Ordering::Equal;
        }
        let (lo, hi) = if largest { (&b.0, &a.0) } else { (&a.0, &b.0) };
        match compare_lt(lo, hi) {
            Ok(true) => std::cmp::Ordering::Less,
            Ok(false) => match compare_lt(hi, lo) {
                Ok(true) => std::cmp::Ordering::Greater,
                Ok(false) => std::cmp::Ordering::Equal,
                Err(e) => {
                    err = Some(e);
                    std::cmp::Ordering::Equal
                }
            },
            Err(e) => {
                err = Some(e);
                std::cmp::Ordering::Equal
            }
        }
    });
    if let Some(e) = err {
        return Err(e);
    }
    let take = usize::try_from(n).unwrap_or(usize::MAX).min(decorated.len());
    Ok(Value::List(shared_list(decorated.into_iter().take(take).map(|(_, v)| v).collect())))
}

/// Sort a `Vec<Value>` by the sync `<` comparator, propagating a
/// `TypeError` on the first uncomparable pair.
fn sort_values(values: &mut [Value], reverse: bool) -> Result<(), EvalError> {
    let mut err: Option<EvalError> = None;
    values.sort_by(|a, b| {
        if err.is_some() {
            return std::cmp::Ordering::Equal;
        }
        let (lo, hi) = if reverse { (b, a) } else { (a, b) };
        match compare_lt(lo, hi) {
            Ok(true) => std::cmp::Ordering::Less,
            Ok(false) => match compare_lt(hi, lo) {
                Ok(true) => std::cmp::Ordering::Greater,
                Ok(false) => std::cmp::Ordering::Equal,
                Err(e) => {
                    err = Some(e);
                    std::cmp::Ordering::Equal
                }
            },
            Err(e) => {
                err = Some(e);
                std::cmp::Ordering::Equal
            }
        }
    });
    err.map_or(Ok(()), Err)
}
