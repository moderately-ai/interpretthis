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
// EvalError used for BigInt method overflow path.

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

/// Reject any keyword arguments. Use for methods that take only positionals
/// (or no args) when the caller passed kwargs — CPython raises TypeError
/// rather than silently ignoring them.
pub(crate) fn reject_kwargs(
    method: &str,
    kwargs: &IndexMap<String, Value>,
) -> Result<(), EvalError> {
    if let Some((name, _)) = kwargs.first() {
        return Err(InterpreterError::TypeError(format!(
            "{method}() got an unexpected keyword argument '{name}'"
        ))
        .into());
    }
    Ok(())
}

/// Bind positional + keyword args onto named method parameters.
///
/// Returns one slot per `params` entry (`None` = not supplied). Enforces:
/// - no more positionals than `params.len()`
/// - no unknown kwargs
/// - no argument supplied both positionally and by keyword
///
/// Callers decide which slots are required and supply defaults for the rest.
pub(crate) fn bind_method_params(
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    params: &[&str],
) -> Result<Vec<Option<Value>>, EvalError> {
    if args.len() > params.len() {
        return Err(InterpreterError::TypeError(format!(
            "{method}() takes at most {} argument{} ({} given)",
            params.len(),
            if params.len() == 1 { "" } else { "s" },
            args.len()
        ))
        .into());
    }
    let mut bound: Vec<Option<Value>> = params.iter().map(|_| None).collect();
    for (i, arg) in args.iter().enumerate() {
        bound[i] = Some(arg.clone());
    }
    for (name, value) in kwargs {
        let Some(idx) = params.iter().position(|p| *p == name.as_str()) else {
            return Err(InterpreterError::TypeError(format!(
                "{method}() got an unexpected keyword argument '{name}'"
            ))
            .into());
        };
        if bound[idx].is_some() {
            return Err(InterpreterError::TypeError(format!(
                "{method}() got multiple values for argument '{name}'"
            ))
            .into());
        }
        bound[idx] = Some(value.clone());
    }
    Ok(bound)
}

/// Require a bound slot (positional or keyword) by index.
pub(crate) fn require_param<'a>(
    method: &str,
    bound: &'a [Option<Value>],
    idx: usize,
    name: &str,
) -> Result<&'a Value, EvalError> {
    bound.get(idx).and_then(Option::as_ref).ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!(
            "{method}() missing required argument: '{name}'"
        )))
    })
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

/// Resolve lazy-proxy values nested in keyword arguments.
pub(super) async fn resolve_method_kwargs(
    kwargs: &IndexMap<String, Value>,
) -> Result<IndexMap<String, Value>, EvalError> {
    let mut resolved = IndexMap::with_capacity(kwargs.len());
    for (k, v) in kwargs {
        resolved.insert(k.clone(), resolve_proxy(v).await?);
    }
    Ok(resolved)
}

/// Dispatch a method call against a mutable receiver slot.
///
/// Per-type method tables live here (keyed by the builtin type name from
/// [`crate::types::type_name_of`]); `TypeObject::has_methods_table` marks
/// which builtins participate. Read-only methods return a fresh value
/// (`mem_delta == 0`); mutating methods modify `obj` in place and report
/// the byte delta. `args` / `kwargs` must already be proxy-resolved
/// (see [`resolve_method_args`] / [`resolve_method_kwargs`]).
pub(super) fn dispatch_method(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    // Prefer TypeObject::has_methods_table when the value is on the
    // type-object table; module-backed variants (ReMatch, HashDigest
    // already flagged, Date*) still dispatch via the match below.
    match obj {
        Value::String(s) => {
            methods::str::dispatch_string_method(s, method, args, kwargs).map(MethodOutcome::pure)
        }
        Value::List(items) => {
            // Lock the shared list for the duration of the method call —
            // list methods (`append`, `pop`, `sort`, `reverse`, slice
            // assigns, …) all need exclusive mutation over the inner Vec.
            // The guard's scope is bounded by the dispatch return.
            let mut guard = items.lock();
            methods::list::dispatch_list_method(&mut guard, method, args, kwargs)
        }
        Value::Dict(map) => methods::dict::dispatch_dict_method(map, method, args, kwargs),
        Value::Counter(map) => methods::counter::dispatch_counter_method(map, method, args, kwargs),
        Value::Deque { items, maxlen } => {
            methods::deque::dispatch_deque_method(items, maxlen.as_ref(), method, args, kwargs)
        }
        Value::DefaultDict(data) => {
            // DefaultDict shares dict's method surface for everything
            // except __missing__ (which is the get-path, not a method).
            // Route through dispatch_dict_method against the backing
            // map.
            methods::dict::dispatch_dict_method(&mut data.items, method, args, kwargs)
        }
        Value::Set(items) => methods::set::dispatch_set_method(items, method, args, kwargs),
        Value::Tuple(items) => methods::tuple::dispatch_tuple_method(items, method, args, kwargs)
            .map(MethodOutcome::pure),
        Value::Date(date) => {
            crate::eval::modules::datetime::dispatch_date_method(*date, method, args, kwargs)
                .map(MethodOutcome::pure)
        }
        Value::DateTime { dt, tz_offset_secs } => {
            crate::eval::modules::datetime::dispatch_datetime_method(
                *dt,
                *tz_offset_secs,
                method,
                args,
                kwargs,
            )
            .map(MethodOutcome::pure)
        }
        Value::Time(t) => {
            crate::eval::modules::datetime::dispatch_time_method(*t, method, args, kwargs)
                .map(MethodOutcome::pure)
        }
        Value::TimeDelta(micros) => {
            crate::eval::modules::datetime::dispatch_timedelta_method(*micros, method, args, kwargs)
                .map(MethodOutcome::pure)
        }
        Value::ReMatch(m) => {
            crate::eval::modules::re::dispatch_match_method(m, method, args, kwargs)
                .map(MethodOutcome::pure)
        }
        Value::HashDigest { algo, bytes } => {
            crate::eval::modules::hashlib::dispatch_hash_method(algo, bytes, method, args, kwargs)
                .map(MethodOutcome::pure)
        }
        Value::Int(i) => {
            methods::int::dispatch_int_method(*i, method, args, kwargs).map(MethodOutcome::pure)
        }
        Value::BigInt(i) => {
            // int methods that need i64 (bit_length, etc.) narrow or error.
            match i64::try_from(i.as_ref()) {
                Ok(n) => methods::int::dispatch_int_method(n, method, args, kwargs)
                    .map(MethodOutcome::pure),
                Err(_) => Err(EvalError::Exception(crate::value::ExceptionValue::new(
                    "OverflowError",
                    "Python int too large to convert to C long",
                ))),
            }
        }
        Value::Bytes(b) => {
            methods::bytes::dispatch_bytes_method(b, method, args, kwargs).map(MethodOutcome::pure)
        }
        // A function/lambda stored in a variable then called as `f.attr()` lands
        // here, as does any other non-method-bearing type.
        other => {
            debug_assert!(
                !crate::types::type_has_methods_table(other),
                "type {} claims has_methods_table but has no match arm",
                crate::types::type_name_of(other)
            );
            Err(InterpreterError::AttributeError(format!(
                "'{}' object has no attribute '{method}'",
                other.type_name()
            ))
            .into())
        }
    }
}

/// Fetch the single required positional argument for a method, with a Python-
/// style `TypeError` naming the method when it is missing.
pub(crate) fn arg1<'a>(method: &str, args: &'a [Value]) -> Result<&'a Value, EvalError> {
    args.first().ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!("{method}() takes exactly 1 argument")))
    })
}
