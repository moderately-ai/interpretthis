// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Unified async dispatch entry points for Python operators.
//!
//! Every operator surface (`obj[k]`, `obj[k] = v`, `del obj[k]`,
//! `for x in obj`, `len(obj)`, `x in obj`, `bool(obj)`, …) has one
//! async entry point here. The entry point:
//!
//! 1. If the lhs / container / value is a `Value::Instance` and the class defines the corresponding
//!    dunder slot, call it via `call_method` and return the slot's result (or its truthiness, for
//!    predicate-shaped ops).
//! 2. Otherwise fall through to the per-type-object slot table in `crate::types` for the
//!    builtin-pair fast path.
//! 3. If neither applies, the slot table raises the appropriate CPython-shaped TypeError.
//!
//! This replaces the per-site instance-slot intercepts that
//! previously lived in `eval_binop`, `eval_compare`, `eval_for`,
//! `eval_subscript`, `eval_assign`, `eval_delete`, etc. Each call
//! site now reads as a single op::X invocation; the dunder lookup
//! and method dispatch live in one place.
//!
//! Predicate-shaped ops (`truthy`, `contains`) return `bool`;
//! materialising ops (`iter`) return `Vec<Value>`; mutators return
//! their per-op shape (`setitem` returns nothing, `delitem` returns
//! the memory delta). The shape of each function follows what the
//! call site actually needs — there is no uniform "Op trait" for the
//! sake of uniformity.

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{
        classes::{call_method, lookup_method_in_mro},
        functions::CallArgs,
    },
    state::InterpreterState,
    tools::Tools,
    value::Value,
};

/// Look up `slot_name` on `value` as a user-class dunder slot. Returns
/// the bound method definition if the value is an Instance and the
/// class (or any MRO ancestor) defines the slot.
fn instance_slot(
    state: &InterpreterState,
    value: &Value,
    slot_name: &str,
) -> Option<crate::value::FunctionDef> {
    let Value::Instance(inst) = value else { return None };
    lookup_method_in_mro(state, &inst.class_name, slot_name).map(|(_, method)| method)
}

/// Invoke a Python slot method on a receiver with positional args.
/// Returns the slot's return value plus the post-call self (which the
/// caller may write back to the source binding for mutating ops).
async fn invoke_slot(
    state: &mut InterpreterState,
    receiver: &Value,
    method: &crate::value::FunctionDef,
    args: &[Value],
    tools: &Tools,
) -> Result<(Value, Value), EvalError> {
    let call = CallArgs { positional: args, keyword: &IndexMap::new() };
    call_method(state, method, receiver.clone(), call, tools).await
}

// ---------------------------------------------------------------------------
// Subscript protocol — getitem / setitem / delitem.
// ---------------------------------------------------------------------------

/// `container[key]` — dispatches `__getitem__` on user-class instances,
/// falls through to `types::dispatch_getitem` for builtin containers.
///
/// When the container is a Dict and the key is a user-class Instance,
/// we compute the key via `op::key` (which dispatches `__hash__`)
/// before the sync dict lookup. Without this, `d[CustomKey()]` would
/// fail at the sync `value_to_key` step inside `dispatch_getitem`.
pub async fn getitem(
    state: &mut InterpreterState,
    container: &Value,
    index: &Value,
    tools: &Tools,
) -> EvalResult {
    if let Some(method) = instance_slot(state, container, "__getitem__") {
        let (returned, _self) =
            invoke_slot(state, container, &method, std::slice::from_ref(index), tools).await?;
        return Ok(returned);
    }
    if let (Value::Dict(map), Value::Instance(_)) = (container, index) {
        return match dict_get_instance_key(state, map, index, tools).await? {
            Some(v) => Ok(v),
            None => {
                let k = key(state, index, tools).await?;
                Err(EvalError::Exception(crate::value::ExceptionValue::key_error(&k)))
            }
        };
    }
    crate::types::dispatch_getitem(container, index)
}

/// `container[key] = value` for a user-class instance: dispatches
/// `__setitem__` and returns the post-call self so the caller can
/// write it back to the source binding. Returns `Ok(None)` if the
/// container is not an Instance (or has no `__setitem__` slot) —
/// the caller is then responsible for the builtin path through the
/// place machinery, which handles nested targets in a way that's
/// expensive to replicate here.
pub async fn setitem(
    state: &mut InterpreterState,
    container: &Value,
    key: &Value,
    value: Value,
    tools: &Tools,
) -> Result<Option<Value>, EvalError> {
    let Some(method) = instance_slot(state, container, "__setitem__") else {
        return Ok(None);
    };
    let (_returned, updated_self) =
        invoke_slot(state, container, &method, &[key.clone(), value], tools).await?;
    Ok(Some(updated_self))
}

/// `del container[key]` for a user-class instance. Same shape as
/// `setitem` — returns `Ok(Some(post-call self))` when a slot
/// dispatched, `Ok(None)` when no slot exists.
pub async fn delitem(
    state: &mut InterpreterState,
    container: &Value,
    key: &Value,
    tools: &Tools,
) -> Result<Option<Value>, EvalError> {
    let Some(method) = instance_slot(state, container, "__delitem__") else {
        return Ok(None);
    };
    let (_returned, updated_self) =
        invoke_slot(state, container, &method, std::slice::from_ref(key), tools).await?;
    Ok(Some(updated_self))
}

// ---------------------------------------------------------------------------
// Iteration protocol — iter / next.
// ---------------------------------------------------------------------------

/// If `value` is a `collections.namedtuple` instance (class has a
/// `_fields` tuple attr), return field values in declaration order.
/// Used for iteration / `len` so namedtuples behave like tuples.
fn namedtuple_items(state: &InterpreterState, value: &Value) -> Option<Vec<Value>> {
    let Value::Instance(inst) = value else {
        return None;
    };
    let class = state.classes.get(&inst.class_name)?;
    let Value::Tuple(field_names) = class.class_attrs.get("_fields")? else {
        return None;
    };
    let mut items = Vec::with_capacity(field_names.len());
    for name in field_names {
        let Value::String(fname) = name else {
            return None;
        };
        items.push(inst.fields.get(fname.as_str()).cloned().unwrap_or(Value::None));
    }
    Some(items)
}

/// Materialise an iterable into a `Vec<Value>`. Dispatches
/// `__iter__`/`__next__` on user-class instances (capped by the
/// configured while-iteration budget so a runaway iterator fails
/// LimitExceeded rather than hanging); falls through to the sync
/// `types::dispatch_iter` for builtin iterables.
pub async fn iter(
    state: &mut InterpreterState,
    value: &Value,
    tools: &Tools,
) -> Result<Vec<Value>, EvalError> {
    // Generator iterators: yield items[cursor..] and advance the
    // cursor to end. Subsequent iter() calls return empty (matching
    // CPython's "a generator can be iterated only once").
    if let Value::Lazy { items, cursor_id } = value {
        let cursor = state.lazy_cursors.get(cursor_id).copied().unwrap_or(0);
        let remaining: Vec<Value> = items.iter().skip(cursor).cloned().collect();
        state.lazy_cursors.insert(*cursor_id, items.len());
        return Ok(remaining);
    }
    // namedtuple: iterate field values in `_fields` order (CPython
    // inherits tuple iteration). Checked before generic Instance
    // `__iter__` so a bare namedtuple without a user `__iter__` works.
    if let Some(items) = namedtuple_items(state, value) {
        return Ok(items);
    }
    let Some(iter_method) = instance_slot(state, value, "__iter__") else {
        return crate::types::dispatch_iter(value);
    };

    let (iterator, _self) = invoke_slot(state, value, &iter_method, &[], tools).await?;
    let Value::Instance(iter_inst) = &iterator else {
        return Err(InterpreterError::TypeError(format!(
            "iter() returned non-iterator of type '{}'",
            iterator.type_name()
        ))
        .into());
    };
    let next_method = lookup_method_in_mro(state, &iter_inst.class_name, "__next__")
        .map(|(_, m)| m)
        .ok_or_else(|| {
            InterpreterError::TypeError(format!(
                "iter() returned non-iterator of type '{}' (no __next__)",
                iter_inst.class_name
            ))
        })?;

    let max_iters = state.config.max_while_iterations;
    let mut items: Vec<Value> = Vec::new();
    let mut iterator_value = iterator;
    for _ in 0..max_iters {
        let next_result = invoke_slot(state, &iterator_value, &next_method, &[], tools).await;
        match next_result {
            Ok((item, updated_self)) => {
                items.push(item);
                iterator_value = updated_self;
            }
            Err(EvalError::Exception(exc)) if exc.type_name == "StopIteration" => {
                return Ok(items);
            }
            Err(other) => return Err(other),
        }
    }
    Err(InterpreterError::LimitExceeded(format!(
        "iterator exceeded maximum iterations ({max_iters})"
    ))
    .into())
}

// ---------------------------------------------------------------------------
// Binary operators — arithmetic + bitwise with __op__ / __rop__ slots.
// ---------------------------------------------------------------------------

/// Apply an augmented binary operator (`x += y`, `x *= y`, …).
/// Dispatches the in-place slot (`__iadd__` / `__imul__` / …) on a
/// user-class lhs first. Per CPython, when the in-place slot is
/// absent the operation falls back to the non-in-place form
/// (`__add__` etc.) — the caller then rebinds the target to the new
/// value rather than mutating in place. Builtin pairs route through
/// the sync `apply_binop` kernel.
pub async fn aug_binop(
    state: &mut InterpreterState,
    op: rustpython_parser::ast::Operator,
    left: &Value,
    right: &Value,
    tools: &Tools,
) -> EvalResult {
    if let Some(method) = instance_slot(state, left, inplace_arith_slot(op)) {
        let (returned, _self) =
            invoke_slot(state, left, &method, std::slice::from_ref(right), tools).await?;
        return Ok(returned);
    }
    binop(state, op, left, right, tools).await
}

const fn inplace_arith_slot(op: rustpython_parser::ast::Operator) -> &'static str {
    use rustpython_parser::ast::Operator;
    match op {
        Operator::Add => "__iadd__",
        Operator::Sub => "__isub__",
        Operator::Mult => "__imul__",
        Operator::Div => "__itruediv__",
        Operator::FloorDiv => "__ifloordiv__",
        Operator::Mod => "__imod__",
        Operator::Pow => "__ipow__",
        Operator::MatMult => "__imatmul__",
        Operator::LShift => "__ilshift__",
        Operator::RShift => "__irshift__",
        Operator::BitOr => "__ior__",
        Operator::BitXor => "__ixor__",
        Operator::BitAnd => "__iand__",
    }
}

/// Apply a binary operator. Dispatches the matching forward slot
/// (`__add__` / `__sub__` / …) on a user-class lhs; falls back to the
/// reflected slot (`__radd__` / `__rsub__` / …) on the rhs when the
/// lhs has no forward slot. Builtin pairs route through the
/// sync `apply_binop` kernel.
pub async fn binop(
    state: &mut InterpreterState,
    op: rustpython_parser::ast::Operator,
    left: &Value,
    right: &Value,
    tools: &Tools,
) -> EvalResult {
    if let Some(method) = instance_slot(state, left, arith_slot(op)) {
        let (returned, _self) =
            invoke_slot(state, left, &method, std::slice::from_ref(right), tools).await?;
        return Ok(returned);
    }
    if let Some(method) = instance_slot(state, right, reflected_arith_slot(op)) {
        let (returned, _self) =
            invoke_slot(state, right, &method, std::slice::from_ref(left), tools).await?;
        return Ok(returned);
    }
    crate::eval::operations::apply_binop(left, right, op)
}

const fn arith_slot(op: rustpython_parser::ast::Operator) -> &'static str {
    use rustpython_parser::ast::Operator;
    match op {
        Operator::Add => "__add__",
        Operator::Sub => "__sub__",
        Operator::Mult => "__mul__",
        Operator::Div => "__truediv__",
        Operator::FloorDiv => "__floordiv__",
        Operator::Mod => "__mod__",
        Operator::Pow => "__pow__",
        Operator::MatMult => "__matmul__",
        Operator::LShift => "__lshift__",
        Operator::RShift => "__rshift__",
        Operator::BitOr => "__or__",
        Operator::BitXor => "__xor__",
        Operator::BitAnd => "__and__",
    }
}

const fn reflected_arith_slot(op: rustpython_parser::ast::Operator) -> &'static str {
    use rustpython_parser::ast::Operator;
    match op {
        Operator::Add => "__radd__",
        Operator::Sub => "__rsub__",
        Operator::Mult => "__rmul__",
        Operator::Div => "__rtruediv__",
        Operator::FloorDiv => "__rfloordiv__",
        Operator::Mod => "__rmod__",
        Operator::Pow => "__rpow__",
        Operator::MatMult => "__rmatmul__",
        Operator::LShift => "__rlshift__",
        Operator::RShift => "__rrshift__",
        Operator::BitOr => "__ror__",
        Operator::BitXor => "__rxor__",
        Operator::BitAnd => "__rand__",
    }
}

// ---------------------------------------------------------------------------
// Rich comparison — __eq__ / __lt__ / __le__ / __gt__ / __ge__.
// ---------------------------------------------------------------------------

/// Evaluate a rich-compare op (`==`, `!=`, `<`, `<=`, `>`, `>=`) on
/// two values. User-class instances dispatch the matching dunder slot
/// (with the reflected slot as fallback when the lhs has none); builtin
/// pairs route through the sync `dispatch_eq` / `dispatch_lt` slot
/// table. Identity (`is` / `is not`) and membership (`in` / `not in`)
/// are NOT handled here — they're either trivial (identity) or have
/// their own `op::contains` entry point.
/// Returns the boolean comparison result plus the post-slot LHS and
/// RHS instance values when a user-class slot ran. Callers (e.g.
/// `eval_compare`) write these back to the originating variable
/// names so attribute mutations performed inside `__lt__` / `__eq__`
/// etc. don't get dropped after the expression evaluates.
pub async fn compare(
    state: &mut InterpreterState,
    op: rustpython_parser::ast::CmpOp,
    left: &Value,
    right: &Value,
    tools: &Tools,
) -> Result<(bool, Option<Value>, Option<Value>), EvalError> {
    use rustpython_parser::ast::CmpOp;
    if let Some(slot) = forward_compare_slot(op) {
        if let Some(method) = instance_slot(state, left, slot) {
            let (returned, post_self) =
                invoke_slot(state, left, &method, std::slice::from_ref(right), tools).await?;
            let result = if matches!(op, CmpOp::NotEq) {
                !returned.is_truthy()
            } else {
                returned.is_truthy()
            };
            return Ok((result, Some(post_self), None));
        }
        if let Some(reflected) = reflected_compare_slot(op) {
            if let Some(method) = instance_slot(state, right, reflected) {
                let (returned, post_self) =
                    invoke_slot(state, right, &method, std::slice::from_ref(left), tools).await?;
                return Ok((returned.is_truthy(), None, Some(post_self)));
            }
        }
    }
    let r = crate::eval::operations::compare_builtin(state, op, left, right)?;
    Ok((r, None, None))
}

/// Async-aware `<` for sorted / min / max. Dispatches `__lt__` on a
/// user-class lhs, then reflected `__gt__` on the rhs, then the sync
/// builtin `dispatch_lt`.
pub async fn lt(
    state: &mut InterpreterState,
    left: &Value,
    right: &Value,
    tools: &Tools,
) -> Result<bool, EvalError> {
    if let Some(method) = instance_slot(state, left, "__lt__") {
        let (returned, _self) =
            invoke_slot(state, left, &method, std::slice::from_ref(right), tools).await?;
        return Ok(returned.is_truthy());
    }
    if let Some(method) = instance_slot(state, right, "__gt__") {
        let (returned, _self) =
            invoke_slot(state, right, &method, std::slice::from_ref(left), tools).await?;
        return Ok(returned.is_truthy());
    }
    crate::types::dispatch_lt(left, right)
}

const fn forward_compare_slot(op: rustpython_parser::ast::CmpOp) -> Option<&'static str> {
    use rustpython_parser::ast::CmpOp;
    match op {
        CmpOp::Eq | CmpOp::NotEq => Some("__eq__"),
        CmpOp::Lt => Some("__lt__"),
        CmpOp::LtE => Some("__le__"),
        CmpOp::Gt => Some("__gt__"),
        CmpOp::GtE => Some("__ge__"),
        _ => None,
    }
}

const fn reflected_compare_slot(op: rustpython_parser::ast::CmpOp) -> Option<&'static str> {
    use rustpython_parser::ast::CmpOp;
    match op {
        CmpOp::Lt => Some("__gt__"),
        CmpOp::LtE => Some("__ge__"),
        CmpOp::Gt => Some("__lt__"),
        CmpOp::GtE => Some("__le__"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Hash / key construction — __hash__ for user classes.
// ---------------------------------------------------------------------------

/// Compute `hash(value)`. User-class instances dispatch `__hash__`
/// and the return value is coerced to `i64`; non-Instance values
/// route through the sync `dispatch_hash` slot table. The state
/// borrow accepts `&InterpreterState` so callers from the sync hash
/// path (which need it for class-registry lookups in the
/// `instance_eq` shortcut) keep working.
pub async fn hash(
    state: &mut InterpreterState,
    value: &Value,
    tools: &Tools,
) -> Result<i64, EvalError> {
    if let Some(method) = instance_slot(state, value, "__hash__") {
        let (returned, _self) = invoke_slot(state, value, &method, &[], tools).await?;
        return match returned {
            Value::Int(n) => Ok(n),
            Value::Bool(b) => Ok(i64::from(b)),
            other => Err(InterpreterError::TypeError(format!(
                "__hash__ method should return an integer, returned {}",
                other.type_name()
            ))
            .into()),
        };
    }
    crate::types::dispatch_hash(state, value)
}

/// Convert a `Value` to a `ValueKey` for dict/set storage,
/// dispatching `__hash__` on user-class instances. Falls through to
/// the sync `value_to_key` for non-Instance values (which materialises
/// the existing builtin-key folds — bool↔int unification, integral-
/// float fold, tuple recursion).
pub async fn key(
    state: &mut InterpreterState,
    value: &Value,
    tools: &Tools,
) -> Result<crate::value::ValueKey, EvalError> {
    if matches!(value, Value::Instance(_)) {
        let h = hash(state, value, tools).await?;
        return Ok(crate::value::ValueKey::Instance { hash: h, value: Box::new(value.clone()) });
    }
    crate::eval::literals::value_to_key(value)
}

/// Async equality for values that may be user-class instances with a
/// custom `__eq__`. Builtins fall through to `dispatch_eq`.
pub async fn eq(
    state: &mut InterpreterState,
    left: &Value,
    right: &Value,
    tools: &Tools,
) -> Result<bool, EvalError> {
    use rustpython_parser::ast::CmpOp;
    let (result, _, _) = compare(state, CmpOp::Eq, left, right, tools).await?;
    Ok(result)
}

/// Look up `needle` in a dict whose keys may include user-class
/// instances. Hash via `__hash__`, then match entries with equal hash
/// using async `__eq__` (IndexMap's `Eq` is structural-only for
/// Instance keys and cannot run user dunders).
async fn dict_get_instance_key(
    state: &mut InterpreterState,
    map: &indexmap::IndexMap<crate::value::ValueKey, Value>,
    needle: &Value,
    tools: &Tools,
) -> Result<Option<Value>, EvalError> {
    let h = hash(state, needle, tools).await?;
    for (k, v) in map {
        if let crate::value::ValueKey::Instance { hash: kh, value } = k {
            if *kh == h && eq(state, value, needle, tools).await? {
                return Ok(Some(v.clone()));
            }
        }
    }
    Ok(None)
}

/// Insert-or-replace for an Instance dict key using `__hash__` + `__eq__`.
pub async fn dict_insert_instance_key_pub(
    state: &mut InterpreterState,
    map: &mut indexmap::IndexMap<crate::value::ValueKey, Value>,
    needle: &Value,
    value: Value,
    tools: &Tools,
) -> Result<(), EvalError> {
    let h = hash(state, needle, tools).await?;
    // Replace the first equal-by-eq entry (CPython keeps first-inserted key object).
    let mut replace_at: Option<usize> = None;
    for (idx, (k, _)) in map.iter().enumerate() {
        if let crate::value::ValueKey::Instance { hash: kh, value: stored } = k {
            if *kh == h && eq(state, stored, needle, tools).await? {
                replace_at = Some(idx);
                break;
            }
        }
    }
    if let Some(idx) = replace_at {
        if let Some(entry) = map.get_index_mut(idx) {
            *entry.1 = value;
        }
    } else {
        map.insert(
            crate::value::ValueKey::Instance { hash: h, value: Box::new(needle.clone()) },
            value,
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Predicate protocols — len / contains / truthy.
// ---------------------------------------------------------------------------

/// `len(value)` — dispatches `__len__` on user-class instances (must
/// return an int; non-int returns raise TypeError matching CPython's
/// "object cannot be interpreted as an integer"). Falls through to
/// the sync `types::dispatch_len` for builtin containers.
pub async fn len(
    state: &mut InterpreterState,
    value: &Value,
    tools: &Tools,
) -> Result<usize, EvalError> {
    if let Some(method) = instance_slot(state, value, "__len__") {
        let (returned, _self) = invoke_slot(state, value, &method, &[], tools).await?;
        return match returned {
            Value::Int(n) => usize::try_from(n).map_err(|_| {
                InterpreterError::ValueError("__len__() should return >= 0".into()).into()
            }),
            other => Err(InterpreterError::TypeError(format!(
                "'{}' object cannot be interpreted as an integer",
                other.type_name()
            ))
            .into()),
        };
    }
    if let Some(items) = namedtuple_items(state, value) {
        return Ok(items.len());
    }
    crate::types::dispatch_len(value)
}

/// `item in container` — dispatches `__contains__` on user-class
/// instances (result interpreted via truthiness); falls through to
/// `types::dispatch_contains` for builtin containers.
///
/// (Dict, Instance) and (Set, Instance) pairs need an async key
/// computation before the sync membership test — same reason as
/// `getitem`.
pub async fn contains(
    state: &mut InterpreterState,
    container: &Value,
    item: &Value,
    tools: &Tools,
) -> Result<bool, EvalError> {
    if let Some(method) = instance_slot(state, container, "__contains__") {
        let (returned, _self) =
            invoke_slot(state, container, &method, std::slice::from_ref(item), tools).await?;
        return Ok(returned.is_truthy());
    }
    if matches!(item, Value::Instance(_)) {
        if let Value::Dict(map) = container {
            return Ok(dict_get_instance_key(state, map, item, tools).await?.is_some());
        }
        // list / tuple / set: scan with async `__eq__` (structural
        // `values_equal` misses custom equality logic).
        if let Value::List(items) = container {
            let snapshot = items.lock().clone();
            for stored in &snapshot {
                if eq(state, stored, item, tools).await? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        if let Value::Tuple(items) | Value::Set(items) = container {
            for stored in items {
                if eq(state, stored, item, tools).await? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
    }
    crate::types::dispatch_contains(container, item)
}

/// Try to compute truthiness synchronously, without allocating a
/// `Box::pin`'d future. Returns `Some(b)` for every shape that doesn't
/// need to await user-class `__bool__` / `__len__` dispatch.
///
/// `Value::Instance` is the only shape that can return `None` — every
/// builtin's truthiness is the sync `Value::is_truthy` table lookup.
/// Hot callers (`if`-clauses inside comprehensions, `while`/`if`
/// conditions on plain values) try this first and avoid the future box
/// on the common case.
#[inline]
#[must_use]
pub fn try_truthy_sync(value: &Value) -> Option<bool> {
    match value {
        Value::Instance(_) => None,
        other => Some(other.is_truthy()),
    }
}

/// `bool(value)` / `if value:` — dispatches `__bool__` then `__len__`
/// on user-class instances; falls through to the sync `Value::is_truthy`
/// for builtins (None / 0 / "" / [] / etc. are falsy; everything else
/// truthy).
pub async fn truthy(
    state: &mut InterpreterState,
    value: &Value,
    tools: &Tools,
) -> Result<bool, EvalError> {
    if matches!(value, Value::Instance(_)) {
        if let Some(method) = instance_slot(state, value, "__bool__") {
            let (returned, _self) = invoke_slot(state, value, &method, &[], tools).await?;
            return match returned {
                Value::Bool(b) => Ok(b),
                other => Err(InterpreterError::TypeError(format!(
                    "__bool__ should return bool, returned {}",
                    other.type_name()
                ))
                .into()),
            };
        }
        if let Some(method) = instance_slot(state, value, "__len__") {
            let (returned, _self) = invoke_slot(state, value, &method, &[], tools).await?;
            return match returned {
                Value::Int(n) => Ok(n != 0),
                Value::Bool(b) => Ok(b),
                other => Err(InterpreterError::TypeError(format!(
                    "'{}' object cannot be interpreted as an integer",
                    other.type_name()
                ))
                .into()),
            };
        }
    }
    Ok(value.is_truthy())
}
