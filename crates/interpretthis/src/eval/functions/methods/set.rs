// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `set` method dispatch — union/intersection/difference/symmetric_difference/
//! issubset/issuperset/isdisjoint plus mutating add/remove/discard/pop/clear/
//! update and the `*_update` variants. Sets carry a CPython-order hash table
//! ([`crate::pyset::SetBody`]): a set-vs-set operation runs CPython's
//! presize/merge table algebra (so the result's iteration and `pop` order
//! match), while a set-vs-other-iterable operation adds or discards
//! element-by-element — the same split CPython's `setobject.c` makes.

use std::sync::Arc;

use super::super::{MethodOutcome, arg1};
use crate::{
    error::{EvalError, InterpreterError},
    eval::{control_flow::iterate_value, literals::value_to_key},
    pyset::SetBody,
    state::estimate_value_size,
    value::{ExceptionValue, SharedSet, Value},
};

/// Wrap a computed body in a fresh `set` value.
fn set_value(body: SetBody) -> Value {
    Value::Set(crate::value::shared_set(body))
}

/// A method argument as a set body: a set/frozenset yields a snapshot of its
/// body; any other iterable is built incrementally (CPython
/// `make_new_set_basetype(other)`). Locks the argument only here, with the
/// receiver lock already released, so `s.method(s)` cannot re-lock the mutex.
fn arg_body(arg: &Value) -> Result<SetBody, EvalError> {
    match arg {
        Value::Set(s) => Ok(s.lock().clone()),
        Value::Frozenset(f) => Ok((**f).clone()),
        other => Ok(SetBody::from_items(iterate_value(other)?)),
    }
}

/// Fold one `union`/`update` argument into `acc`: a set/frozenset merges
/// (CPython `set_merge`, presized), any other iterable adds element-by-element.
fn apply_union(acc: &mut SetBody, arg: &Value) -> Result<(), EvalError> {
    match arg {
        Value::Set(s) => acc.merge_from(&s.lock()),
        Value::Frozenset(f) => acc.merge_from(f),
        other => {
            for item in iterate_value(other)? {
                acc.add_value(item);
            }
        }
    }
    Ok(())
}

/// `acc ∩ arg` as a new body: a set intersects via the table op (iterate the
/// smaller); any other iterable is iterated in its own order, keeping the
/// elements present in `acc` — CPython `set_intersection`.
fn intersect_arg(acc: &SetBody, arg: &Value) -> Result<SetBody, EvalError> {
    match arg {
        Value::Set(s) => Ok(acc.intersection_with(&s.lock())),
        Value::Frozenset(f) => Ok(acc.intersection_with(f)),
        other => {
            let mut r = SetBody::empty();
            for item in iterate_value(other)? {
                if acc.contains(&item) {
                    r.add_value(item);
                }
            }
            Ok(r)
        }
    }
}

/// Fold one `difference`/`difference_update` argument into `acc` in place: a
/// set uses the table op (tombstone-resize included), any other iterable
/// discards element-by-element then compacts — CPython
/// `set_difference_update_internal`.
fn apply_difference(acc: &mut SetBody, arg: &Value) -> Result<(), EvalError> {
    match arg {
        Value::Set(s) => acc.difference_from(&s.lock()),
        Value::Frozenset(f) => acc.difference_from(f),
        other => {
            let items = iterate_value(other)?;
            acc.difference_from(&SetBody::from_items(items));
        }
    }
    Ok(())
}

/// Estimated element bytes of a body, for the memory-accounting delta.
fn body_bytes(body: &SetBody) -> usize {
    body.iter_ordered().iter().map(estimate_value_size).sum()
}

/// Outcome for a bulk mutation, reporting the signed change in element bytes.
fn delta_outcome(old: usize, new: usize) -> MethodOutcome {
    if new >= old {
        MethodOutcome::grew(Value::None, new - old)
    } else {
        MethodOutcome::shrank(Value::None, old - new)
    }
}

/// Dispatch a `set` method. Takes the shared handle, not a held guard: every
/// operation snapshots the receiver body (lock released) before touching the
/// arguments, so an argument that IS the receiver (`s.update(s)`) can never
/// re-lock the one mutex (the mutex is non-reentrant). Set-vs-set operations go
/// through CPython's presize/merge table algebra (order-faithful); set-vs-other
/// iterables add/discard element-by-element, matching CPython's own split.
pub(crate) fn dispatch_set_method(
    shared: &SharedSet,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    crate::eval::functions::reject_kwargs(method, kwargs)?;

    match method {
        // `set.copy()` is CPython `set_copy` — a presized fresh table, not a
        // verbatim clone (drops tombstones).
        "copy" => Ok(MethodOutcome::pure(set_value(shared.lock().copied()))),
        // union/intersection/difference accept any number of iterable args.
        "union" => {
            let mut acc = shared.lock().copied();
            for arg in args {
                apply_union(&mut acc, arg)?;
            }
            Ok(MethodOutcome::pure(set_value(acc)))
        }
        "intersection" => {
            if args.is_empty() {
                return Ok(MethodOutcome::pure(set_value(shared.lock().copied())));
            }
            let mut acc = shared.lock().clone();
            for arg in args {
                acc = intersect_arg(&acc, arg)?;
            }
            Ok(MethodOutcome::pure(set_value(acc)))
        }
        "difference" => {
            let mut acc = shared.lock().copied();
            for arg in args {
                apply_difference(&mut acc, arg)?;
            }
            Ok(MethodOutcome::pure(set_value(acc)))
        }
        "symmetric_difference" => {
            let other = arg_body(arg1(method, args)?)?;
            let result = shared.lock().symmetric_difference_with(&other);
            Ok(MethodOutcome::pure(set_value(result)))
        }
        "issubset" => {
            let other = arg_body(arg1(method, args)?)?;
            let result = shared.lock().iter_ordered().iter().all(|v| other.contains(v));
            Ok(MethodOutcome::pure(Value::Bool(result)))
        }
        "issuperset" => {
            let other = iterate_value(arg1(method, args)?)?;
            let body = shared.lock();
            let result = other.iter().all(|v| body.contains(v));
            Ok(MethodOutcome::pure(Value::Bool(result)))
        }
        "isdisjoint" => {
            let other = iterate_value(arg1(method, args)?)?;
            let body = shared.lock();
            let result = !other.iter().any(|v| body.contains(v));
            Ok(MethodOutcome::pure(Value::Bool(result)))
        }
        "add" => {
            let arg = arg1(method, args)?;
            // A genuinely-unhashable element (list/dict/set) raises. Instances
            // are hashable by identity in CPython, so they are allowed.
            if !matches!(arg, Value::Instance(_)) {
                value_to_key(arg)?;
            }
            let size = estimate_value_size(arg);
            if shared.lock().add_value(arg.clone()) {
                Ok(MethodOutcome::grew(Value::None, size))
            } else {
                Ok(MethodOutcome::pure(Value::None))
            }
        }
        "remove" => {
            let arg = arg1(method, args)?;
            let freed = estimate_value_size(arg);
            if shared.lock().discard_value(arg) {
                Ok(MethodOutcome::shrank(Value::None, freed))
            } else {
                Err(EvalError::Exception(ExceptionValue::new("KeyError", arg.repr())))
            }
        }
        "discard" => {
            let arg = arg1(method, args)?;
            let freed = estimate_value_size(arg);
            if shared.lock().discard_value(arg) {
                Ok(MethodOutcome::shrank(Value::None, freed))
            } else {
                // discard() on a missing element is a no-op.
                Ok(MethodOutcome::pure(Value::None))
            }
        }
        "pop" => match shared.lock().pop_first() {
            Some(val) => {
                let freed = estimate_value_size(&val);
                Ok(MethodOutcome::shrank(val, freed))
            }
            None => {
                // KeyError renders `message` verbatim, so the string must carry
                // its own repr quotes (CPython: `KeyError('pop from an empty
                // set')` → `'pop from an empty set'`).
                Err(EvalError::Exception(ExceptionValue::new(
                    "KeyError",
                    "'pop from an empty set'",
                )))
            }
        },
        "clear" => {
            let mut body = shared.lock();
            let freed: usize = body.iter_ordered().iter().map(estimate_value_size).sum();
            body.clear();
            Ok(MethodOutcome::shrank(Value::None, freed))
        }
        "update" => {
            // In-place merge keeps the live table (CPython `set_update`), unlike
            // `union` which starts from a copy. Snapshot to compute off-lock,
            // then write back — so an aliasing argument can't deadlock.
            let mut acc = shared.lock().clone();
            let old = body_bytes(&acc);
            for arg in args {
                apply_union(&mut acc, arg)?;
            }
            let new = body_bytes(&acc);
            *shared.lock() = acc;
            Ok(delta_outcome(old, new))
        }
        "intersection_update" => {
            let mut acc = shared.lock().clone();
            let old = body_bytes(&acc);
            for arg in args {
                acc = intersect_arg(&acc, arg)?;
            }
            let new = body_bytes(&acc);
            *shared.lock() = acc;
            Ok(delta_outcome(old, new))
        }
        "difference_update" => {
            let mut acc = shared.lock().clone();
            let old = body_bytes(&acc);
            for arg in args {
                apply_difference(&mut acc, arg)?;
            }
            let new = body_bytes(&acc);
            *shared.lock() = acc;
            Ok(delta_outcome(old, new))
        }
        "symmetric_difference_update" => {
            let other = arg_body(arg1(method, args)?)?;
            let mut acc = shared.lock().clone();
            let old = body_bytes(&acc);
            // Toggle each of `other`'s elements in `other`'s slot order.
            for item in other.iter_ordered() {
                if !acc.discard_value(&item) {
                    acc.add_value(item);
                }
            }
            let new = body_bytes(&acc);
            *shared.lock() = acc;
            Ok(delta_outcome(old, new))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'set' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Non-mutating `frozenset` methods — the set-algebra subset that returns a new
/// value. Delegates to [`dispatch_set_method`] on a clone (so no mutation
/// escapes) and rewraps any `set` result as a `frozenset`. Mutating method
/// names raise `AttributeError`, matching CPython's immutable `frozenset`.
pub(crate) fn dispatch_frozenset_method(
    body: &SetBody,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    const FROZENSET_METHODS: &[&str] = &[
        "copy",
        "union",
        "intersection",
        "difference",
        "symmetric_difference",
        "issubset",
        "issuperset",
        "isdisjoint",
    ];
    if !FROZENSET_METHODS.contains(&method) {
        return Err(InterpreterError::AttributeError(format!(
            "'frozenset' object has no attribute '{method}'"
        ))
        .into());
    }
    // Delegate to the (non-mutating) set methods on a throwaway shared handle.
    let scratch = crate::value::shared_set(body.clone());
    let outcome = dispatch_set_method(&scratch, method, args, kwargs)?;
    // A set result becomes a frozenset, a bool stays a bool.
    let value = match outcome.value {
        Value::Set(v) => Value::Frozenset(Arc::new(v.lock().clone())),
        other => other,
    };
    Ok(MethodOutcome::pure(value))
}
