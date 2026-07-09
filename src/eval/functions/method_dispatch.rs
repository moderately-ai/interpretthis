// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use indexmap::IndexMap;

use super::{methods, resolve_proxy};
use crate::{
    error::{EvalError, InterpreterError},
    eval::place,
    value::{Value, shared_list},
};

/// The positional and keyword arguments of a call, bundled so call-machinery
/// signatures stay under the argument-count limit and the pair always travels
/// together.
#[derive(Clone, Copy)]
pub(crate) struct CallArgs<'a> {
    pub positional: &'a [Value],
    pub keyword: &'a IndexMap<String, Value>,
}

/// Outcome of a method dispatch: the Python return value plus the signed change
/// in the receiver's estimated heap size. The caller applies `mem_delta` to the
/// memory budget once the mutable borrow into `state` has ended, keeping memory
/// accounting O(1) (no re-estimating the whole root after each `append`).
pub(crate) struct MethodOutcome {
    pub value: Value,
    pub mem_delta: isize,
}

impl MethodOutcome {
    /// A non-mutating result (no change to the receiver's size).
    pub(crate) const fn pure(value: Value) -> Self {
        Self { value, mem_delta: 0 }
    }

    /// A mutation that added `bytes` to the receiver.
    pub(crate) fn grew(value: Value, bytes: usize) -> Self {
        Self { value, mem_delta: place::to_isize(bytes) }
    }

    /// A mutation that removed `bytes` from the receiver.
    pub(crate) fn shrank(value: Value, bytes: usize) -> Self {
        Self { value, mem_delta: -place::to_isize(bytes) }
    }
}

/// Resolve lazy-proxy method arguments before dispatch. `join` and friends
/// iterate collection items, so proxies one level inside a list/tuple argument
/// are resolved too.
pub(super) async fn resolve_method_args(args: &[Value]) -> Result<Vec<Value>, EvalError> {
    let mut resolved_args = Vec::with_capacity(args.len());
    for arg in args {
        let resolved = resolve_proxy(arg).await?;
        match resolved {
            Value::List(items) => {
                // Snapshot the items under the lock — `resolve_proxy`
                // may suspend on a tool call, so hold the guard only
                // long enough to clone the inner Vec.
                let snapshot = items.lock().clone();
                let mut resolved_items = Vec::with_capacity(snapshot.len());
                for item in &snapshot {
                    resolved_items.push(resolve_proxy(item).await?);
                }
                resolved_args.push(Value::List(shared_list(resolved_items)));
            }
            Value::Tuple(items) => {
                let mut resolved_items = Vec::with_capacity(items.len());
                for item in &items {
                    resolved_items.push(resolve_proxy(item).await?);
                }
                resolved_args.push(Value::Tuple(resolved_items));
            }
            other => resolved_args.push(other),
        }
    }
    Ok(resolved_args)
}

/// Dispatch a method call against a mutable receiver slot.
///
/// Read-only methods return a fresh value (`mem_delta == 0`); mutating methods
/// modify `obj` in place and report the byte delta. `args` must already be
/// proxy-resolved (see [`resolve_method_args`]).
pub(super) fn dispatch_method(
    obj: &mut Value,
    method: &str,
    args: &[Value],
) -> Result<MethodOutcome, EvalError> {
    match obj {
        Value::String(s) => {
            methods::str::dispatch_string_method(s, method, args).map(MethodOutcome::pure)
        }
        Value::List(items) => {
            // Lock the shared list for the duration of the method call —
            // list methods (`append`, `pop`, `sort`, `reverse`, slice
            // assigns, …) all need exclusive mutation over the inner Vec.
            // The guard's scope is bounded by the dispatch return.
            let mut guard = items.lock();
            methods::list::dispatch_list_method(&mut guard, method, args)
        }
        Value::Dict(map) => methods::dict::dispatch_dict_method(map, method, args),
        Value::Counter(map) => methods::counter::dispatch_counter_method(map, method, args),
        Value::Deque { items, maxlen } => {
            methods::deque::dispatch_deque_method(items, maxlen.as_ref(), method, args)
        }
        Value::DefaultDict(data) => {
            // DefaultDict shares dict's method surface for everything
            // except __missing__ (which is the get-path, not a method).
            // Route through dispatch_dict_method against the backing
            // map.
            methods::dict::dispatch_dict_method(&mut data.items, method, args)
        }
        Value::Set(items) => methods::set::dispatch_set_method(items, method, args),
        Value::Tuple(items) => {
            methods::tuple::dispatch_tuple_method(items, method, args).map(MethodOutcome::pure)
        }
        Value::Date(date) => {
            crate::eval::modules::datetime::dispatch_date_method(*date, method, args)
                .map(MethodOutcome::pure)
        }
        Value::DateTime { dt, tz_offset_secs } => {
            crate::eval::modules::datetime::dispatch_datetime_method(
                *dt,
                *tz_offset_secs,
                method,
                args,
            )
            .map(MethodOutcome::pure)
        }
        Value::Time(t) => crate::eval::modules::datetime::dispatch_time_method(*t, method, args)
            .map(MethodOutcome::pure),
        Value::TimeDelta(micros) => {
            crate::eval::modules::datetime::dispatch_timedelta_method(*micros, method, args)
                .map(MethodOutcome::pure)
        }
        Value::ReMatch(m) => crate::eval::modules::re::dispatch_match_method(m, method, args)
            .map(MethodOutcome::pure),
        Value::HashDigest { algo, bytes } => {
            crate::eval::modules::hashlib::dispatch_hash_method(algo, bytes, method, args)
                .map(MethodOutcome::pure)
        }
        Value::Int(i) => {
            methods::int::dispatch_int_method(*i, method, args).map(MethodOutcome::pure)
        }
        Value::Bytes(b) => {
            methods::bytes::dispatch_bytes_method(b, method, args).map(MethodOutcome::pure)
        }
        // A function/lambda stored in a variable then called as `f.attr()` lands
        // here, as does any other non-method-bearing type.
        _ => Err(InterpreterError::AttributeError(format!(
            "'{}' object has no attribute '{method}'",
            obj.type_name()
        ))
        .into()),
    }
}

/// Fetch the single required positional argument for a method, with a Python-
/// style `TypeError` naming the method when it is missing.
pub(crate) fn arg1<'a>(method: &str, args: &'a [Value]) -> Result<&'a Value, EvalError> {
    args.first().ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!("{method}() takes exactly 1 argument")))
    })
}
