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
    if let (Some(map), Value::Instance(_)) = (container.as_dict(), index) {
        return match dict_get_instance_key(state, map, index, tools).await? {
            Some(v) => Ok(v),
            None => {
                let k = key(state, index, tools).await?;
                Err(EvalError::Exception(crate::value::ExceptionValue::key_error(&k)))
            }
        };
    }
    // `counter[instance]` — async hash/eq lookup; a missing key yields 0
    // (Counter's `__missing__`), never a KeyError.
    if let (Value::Counter(map), Value::Instance(_)) = (container, index) {
        let h = hash(state, index, tools).await?;
        for (k, v) in map {
            if let crate::value::ValueKey::Instance { hash: kh, value } = k {
                if *kh == h && eq(state, value, index, tools).await? {
                    return Ok(v.clone());
                }
            }
        }
        return Ok(Value::Int(0));
    }
    // `Color["RED"]` — an enum class is subscriptable by member name (CPython's
    // `EnumMeta.__getitem__`), raising KeyError for an unknown name.
    if let (Value::Class(class_name), Value::String(member)) = (container, index) {
        if let Some(class) = state.classes.get(class_name) {
            if class.enum_kind.is_some() {
                return class.class_attrs.get(member.as_str()).cloned().ok_or_else(|| {
                    EvalError::Exception(crate::value::ExceptionValue::new(
                        "KeyError",
                        format!("'{member}'"),
                    ))
                });
            }
        }
    }
    // Indexing a sequence with an object that defines `__index__` (CPython's
    // `operator.index`) — e.g. `[10, 20][obj]`. Resolve it to an int, then
    // dispatch normally. Non-sequences (dict/counter) key on the object itself.
    if matches!(index, Value::Instance(_))
        && matches!(
            container,
            Value::List(_)
                | Value::Tuple(_)
                | Value::String(_)
                | Value::Bytes(_)
                | Value::ByteArray(_)
                | Value::Range { .. }
        )
    {
        let coerced = coerce_index(state, index.clone(), tools).await?;
        if !matches!(coerced, Value::Instance(_)) {
            return crate::types::dispatch_getitem(container, &coerced);
        }
    }
    crate::types::dispatch_getitem(container, index)
}

/// Resolve any `__index__`-defining instance to the integer it returns
/// (CPython's `operator.index`), leaving every other value — including an
/// instance without `__index__` — unchanged. Used wherever a sequence index or
/// slice bound accepts an arbitrary `__index__` object.
pub async fn coerce_index(
    state: &mut InterpreterState,
    val: Value,
    tools: &Tools,
) -> Result<Value, EvalError> {
    if matches!(val, Value::Instance(_)) {
        if let Some(resolved) = instance_unary_dunder(state, &val, "__index__", tools).await {
            let idx = resolved?;
            return match idx {
                Value::Int(_) | Value::Bool(_) => Ok(idx),
                other => Err(InterpreterError::TypeError(format!(
                    "__index__ returned non-int (type {})",
                    other.type_name()
                ))
                .into()),
            };
        }
    }
    Ok(val)
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
        items.push(inst.fields.lock().get(fname.as_str()).cloned().unwrap_or(Value::None));
    }
    Some(items)
}

/// Materialise an iterable into a `Vec<Value>`. Dispatches
/// `__iter__`/`__next__` on user-class instances (capped by the
/// configured while-iteration budget so a runaway iterator fails
/// LimitExceeded rather than hanging); falls through to the sync
/// `types::dispatch_iter` for builtin iterables.
async fn drain_generator(
    state: &mut InterpreterState,
    id: u64,
    tools: &Tools,
) -> Result<Vec<Value>, EvalError> {
    Ok(drain_generator_with_return(state, id, tools).await?.0)
}

/// Like [`drain_generator`], but also returns the value the generator's
/// `return` carried in its terminating `StopIteration` (CPython's `e.value`),
/// which a delegating `yield from` uses as its own result. A generator that
/// falls off the end (or `return`s nothing) yields `Value::None`.
pub(crate) async fn drain_generator_with_return(
    state: &mut InterpreterState,
    id: u64,
    tools: &Tools,
) -> Result<(Vec<Value>, Value), EvalError> {
    let mut out = Vec::new();
    loop {
        match crate::eval::functions::dispatch_generator_method(
            state,
            &Value::Generator { id },
            "__next__",
            &[],
            &indexmap::IndexMap::new(),
            tools,
        )
        .await
        {
            Ok(v) => out.push(v),
            Err(EvalError::Exception(exc)) if exc.type_name == "StopIteration" => {
                let ret = exc.args.first().cloned().unwrap_or(Value::None);
                return Ok((out, ret));
            }
            Err(e) => return Err(e),
        }
    }
}

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
    if let Value::Generator { id } = value {
        // Boxed to satisfy async recursion rules (iter → next → body → iter).
        return Box::pin(drain_generator(state, *id, tools)).await;
    }
    // Iterating an enum class yields its members in definition order
    // (`for color in Color`, `list(Color)`).
    if let Value::Class(class_name) = value {
        if let Some(class) = state.classes.get(class_name) {
            if class.enum_kind.is_some() {
                let members = class
                    .enum_members
                    .iter()
                    .filter_map(|name| class.class_attrs.get(name).cloned())
                    .collect();
                return Ok(members);
            }
        }
    }
    // namedtuple: iterate field values in `_fields` order (CPython
    // inherits tuple iteration). Checked before generic Instance
    // `__iter__` so a bare namedtuple without a user `__iter__` works.
    if let Some(items) = namedtuple_items(state, value) {
        return Ok(items);
    }
    let Some(iter_method) = instance_slot(state, value, "__iter__") else {
        // Old-style sequence iteration: a class with `__getitem__` (int
        // indices) but no `__iter__` is iterated by `__getitem__(0)`,
        // `(1)`, … until it raises `IndexError` — CPython's fallback
        // iteration protocol.
        if instance_slot(state, value, "__getitem__").is_some() {
            return Box::pin(getitem_iterate(state, value, tools)).await;
        }
        return crate::types::dispatch_iter(value);
    };

    let (iterator, _self) = invoke_slot(state, value, &iter_method, &[], tools).await?;
    // `__iter__` may return a generator (a generator method's `yield`s) or any
    // builtin iterable; drain those directly rather than requiring an Instance.
    match &iterator {
        Value::Lazy { .. } | Value::Generator { .. } => {
            return Box::pin(iter(state, &iterator, tools)).await;
        }
        Value::List(_) | Value::Tuple(_) | Value::Range { .. } => {
            return Box::pin(iter(state, &iterator, tools)).await;
        }
        _ => {}
    }
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

/// CPython's `__getitem__`-based sequence iteration: index from `0`
/// upward until `__getitem__` raises `IndexError`. Used for a class that
/// defines `__getitem__` but not `__iter__`.
async fn getitem_iterate(
    state: &mut InterpreterState,
    value: &Value,
    tools: &Tools,
) -> Result<Vec<Value>, EvalError> {
    let max_iters = state.config.max_while_iterations;
    let mut items: Vec<Value> = Vec::new();
    for i in 0..max_iters {
        let idx = i64::try_from(i).map_err(|_| {
            EvalError::from(InterpreterError::Runtime("sequence index overflow".into()))
        })?;
        match getitem(state, value, &Value::Int(idx), tools).await {
            Ok(item) => items.push(item),
            Err(EvalError::Exception(exc)) if exc.type_name == "IndexError" => {
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
    // `list += iterable` extends the list in place and returns the *same*
    // handle, so `id(lst)` is preserved (CPython's `list.__iadd__`). Falling
    // through to `binop` would build a fresh list and change identity.
    if let (rustpython_parser::ast::Operator::Add, Value::List(items)) = (op, left) {
        let extension = crate::eval::op::iter(state, right, tools).await?;
        items.lock().extend(extension);
        return Ok(left.clone());
    }
    // `set |= / &= / -= / ^=` mutate the set in place through the shared handle
    // and return it (CPython `set.__ior__`/`__iand__`/`__isub__`/`__ixor__`), so
    // aliases observe the change and identity is preserved — and the order is
    // the in-place `update`/`difference_update`/… order, not the copy-producing
    // binary operator's. These slots accept only a set-like RHS; a non-set RHS
    // falls through to `binop`, which raises just as CPython does. A `frozenset`
    // lhs has no in-place slot, so it also falls through (rebinding to a new
    // frozenset), which is correct.
    {
        use rustpython_parser::ast::Operator as Op;
        if let Value::Set(s) = left {
            if matches!(op, Op::BitOr | Op::BitAnd | Op::Sub | Op::BitXor) {
                // Snapshot the RHS body with the receiver lock released, so
                // `s |= s` (an aliasing RHS) cannot re-lock the one mutex.
                let other = match right {
                    Value::Set(o) => Some(o.lock().clone()),
                    Value::Frozenset(o) => Some((**o).clone()),
                    _ => None,
                };
                if let Some(other) = other {
                    let mut body = s.lock();
                    match op {
                        Op::BitOr => body.merge_from(&other),
                        Op::Sub => body.difference_from(&other),
                        Op::BitAnd => {
                            let intersected = body.intersection_with(&other);
                            *body = intersected;
                        }
                        Op::BitXor => {
                            for v in other.iter_ordered() {
                                if !body.discard_value(&v) {
                                    body.add_value(v);
                                }
                            }
                        }
                        _ => unreachable!(),
                    }
                    drop(body);
                    return Ok(left.clone());
                }
            }
        }
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

/// Combine two `Flag`/`IntFlag` members of the same enum with a bitwise op,
/// producing a member carrying the combined value. Returns `None` when the
/// operands are not same-class flag members.
fn flag_bitwise(
    state: &InterpreterState,
    op: rustpython_parser::ast::Operator,
    left: &Value,
    right: &Value,
) -> Option<EvalResult> {
    use rustpython_parser::ast::Operator;
    let (
        Value::EnumMember { class_name: c1, kind, value: v1, .. },
        Value::EnumMember { class_name: c2, value: v2, .. },
    ) = (left, right)
    else {
        return None;
    };
    if !kind.is_flag() || c1 != c2 {
        return None;
    }
    let a = crate::value::value_as_i64(v1)?;
    let b = crate::value::value_as_i64(v2)?;
    let combined = match op {
        Operator::BitOr => a | b,
        Operator::BitAnd => a & b,
        Operator::BitXor => a ^ b,
        _ => return None,
    };
    Some(Ok(Value::EnumMember {
        class_name: c1.clone(),
        member_name: compose_flag_name(state, c1, combined),
        value: Box::new(Value::Int(combined)),
        kind: *kind,
    }))
}

/// Build the pipe-joined member name for a (possibly composite) flag value,
/// e.g. `R|W` for value 3. `0` renders as the value itself.
fn compose_flag_name(state: &InterpreterState, class_name: &str, value: i64) -> String {
    let Some(class) = state.classes.get(class_name) else {
        return value.to_string();
    };
    let mut parts = Vec::new();
    for member in &class.enum_members {
        if let Some(Value::EnumMember { value: mv, .. }) = class.class_attrs.get(member) {
            if let Some(bit) = crate::value::value_as_i64(mv) {
                if bit != 0 && value & bit == bit {
                    parts.push(member.clone());
                }
            }
        }
    }
    if parts.is_empty() { value.to_string() } else { parts.join("|") }
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
    use rustpython_parser::ast::Operator;
    // Flag / IntFlag members combine bitwise into a new (possibly composite)
    // member of the same enum (`Perm.R | Perm.W`). Handled here so the class
    // registry is available to compose the combined member's name.
    if matches!(op, Operator::BitOr | Operator::BitAnd | Operator::BitXor) {
        if let Some(result) = flag_bitwise(state, op, left, right) {
            return result;
        }
    }
    // `str % args` / `bytes % args` is printf-style formatting handled by
    // `str.__mod__`, which CPython tries before the right operand's `__rmod__`.
    // When an operand is a user-class instance, format on the async path so its
    // numeric/text dunder (`__index__`/`__int__`/`__float__`/`__str__`/…) is
    // dispatched per conversion; builtin-only args stay on the sync fast path.
    if matches!(op, Operator::Mod) {
        match left {
            Value::String(t) if percent_arg_has_instance(right) => {
                return crate::eval::strings::str_percent_format_async(state, t, right, tools)
                    .await;
            }
            Value::Bytes(t) if percent_arg_has_instance(right) => {
                return crate::eval::strings::bytes_percent_format_async(state, t, right, tools)
                    .await;
            }
            Value::ByteArray(t) if percent_arg_has_instance(right) => {
                let snapshot = t.lock().clone();
                return crate::eval::strings::bytes_percent_format_async(
                    state, &snapshot, right, tools,
                )
                .await
                .map(|v| match v {
                    Value::Bytes(b) => Value::ByteArray(crate::value::shared_bytes(b)),
                    other => other,
                });
            }
            _ => {}
        }
    }
    if let Some(method) = instance_slot(state, left, arith_slot(op)) {
        let (returned, _self) =
            invoke_slot(state, left, &method, std::slice::from_ref(right), tools).await?;
        // Returning NotImplemented tries the reflected slot / builtins.
        if !matches!(returned, Value::NotImplemented) {
            return Ok(returned);
        }
    }
    // CPython does not try the right operand's reflected slot when both operands
    // are the *same* type: the reflected method resolves to the same slot that
    // already returned NotImplemented, so `a + b` raises rather than looping back
    // into `a.__radd__(a)`.
    let same_class = matches!(
        (left, right),
        (Value::Instance(a), Value::Instance(b)) if a.class_name == b.class_name
    );
    if !same_class {
        if let Some(method) = instance_slot(state, right, reflected_arith_slot(op)) {
            let (returned, _self) =
                invoke_slot(state, right, &method, std::slice::from_ref(left), tools).await?;
            if !matches!(returned, Value::NotImplemented) {
                return Ok(returned);
            }
        }
    }
    crate::eval::operations::apply_binop(
        left,
        right,
        op,
        state.decimal_prec,
        state.config.max_int_bits,
    )
}

/// Apply a unary operator (`-x`, `+x`, `~x`). Dispatches the matching dunder
/// (`__neg__` / `__pos__` / `__invert__`) on a user-class operand; every other
/// operand routes through the sync `apply_unaryop` kernel. `not x` has no
/// dedicated slot (it derives from `__bool__`), so it always goes to the kernel.
pub async fn unaryop(
    state: &mut InterpreterState,
    op: rustpython_parser::ast::UnaryOp,
    operand: &Value,
    tools: &Tools,
) -> EvalResult {
    use rustpython_parser::ast::UnaryOp;
    let slot = match op {
        UnaryOp::UAdd => Some("__pos__"),
        UnaryOp::USub => Some("__neg__"),
        UnaryOp::Invert => Some("__invert__"),
        UnaryOp::Not => None,
    };
    if let Some(slot) = slot {
        if let Some(method) = instance_slot(state, operand, slot) {
            let (returned, _self) = invoke_slot(state, operand, &method, &[], tools).await?;
            return Ok(returned);
        }
    }
    crate::eval::operations::apply_unaryop(state, op, operand, tools).await
}

/// Whether a `%`-format operand contains a user-class instance (as the whole
/// value, a positional-tuple element, or a mapping value) — the trigger for the
/// async percent-format path that dispatches per-conversion coercion dunders.
fn percent_arg_has_instance(arg: &Value) -> bool {
    match arg {
        Value::Instance(_) => true,
        Value::Tuple(items) => items.iter().any(|v| matches!(v, Value::Instance(_))),
        Value::Dict(d) | Value::OrderedDict(d) => {
            d.lock().values().any(|v| matches!(v, Value::Instance(_)))
        }
        _ => false,
    }
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
            if !matches!(returned, Value::NotImplemented) {
                let result = if matches!(op, CmpOp::NotEq) {
                    !returned.is_truthy()
                } else {
                    returned.is_truthy()
                };
                return Ok((result, Some(post_self), None));
            }
            // NotImplemented → try reflected / builtins below.
        }
        if let Some(reflected) = reflected_compare_slot(op) {
            if let Some(method) = instance_slot(state, right, reflected) {
                let (returned, post_self) =
                    invoke_slot(state, right, &method, std::slice::from_ref(left), tools).await?;
                if !matches!(returned, Value::NotImplemented) {
                    return Ok((returned.is_truthy(), None, Some(post_self)));
                }
            }
        }
    }
    // `@dataclass(order=True)` — field-tuple ordering when neither side
    // defined a custom dunder (above).
    if let Some(r) = dataclass_order_compare(state, op, left, right) {
        return Ok((r?, None, None));
    }
    // `@functools.total_ordering` — derive the missing ordering operator from
    // the one the class defines plus `__eq__`.
    if let Some(r) = total_ordering_derive(state, op, left, right, tools).await? {
        return Ok((r, None, None));
    }
    // An ordering comparison involving a user-class instance that resolved no
    // ordering slot raises directly — CPython does NOT derive `<=`/`>=` from
    // `__lt__`/`__eq__` (only @total_ordering does), so falling through to the
    // builtin kernel (which would derive it, and mislabel `<=` as `<`) is wrong.
    if matches!(op, CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE)
        && (matches!(left, Value::Instance(_)) || matches!(right, Value::Instance(_)))
    {
        let sym = match op {
            CmpOp::Lt => "<",
            CmpOp::LtE => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtE => ">=",
            _ => unreachable!("guarded by the matches! above"),
        };
        return Err(crate::types::type_error_unsupported(sym, left, right));
    }
    let r = crate::eval::operations::compare_builtin(state, op, left, right)?;
    Ok((r, None, None))
}

/// Field-tuple comparison for `@dataclass(order=True)` instances of the
/// same class. Returns `None` when the pair is not an ordered dataclass.
fn dataclass_order_compare(
    state: &InterpreterState,
    op: rustpython_parser::ast::CmpOp,
    left: &Value,
    right: &Value,
) -> Option<Result<bool, EvalError>> {
    use rustpython_parser::ast::CmpOp;
    let (Value::Instance(a), Value::Instance(b)) = (left, right) else {
        return None;
    };
    if a.class_name != b.class_name {
        return None;
    }
    let class = state.classes.get(&a.class_name)?;
    if !class.order {
        return None;
    }
    let fields = class.dataclass_fields.as_ref()?;
    // Build compare-key tuples (fields with compare=True, in order).
    let key_a: Vec<Value> = {
        let af = a.fields.lock();
        fields.iter().filter(|f| f.compare).filter_map(|f| af.get(&f.name).cloned()).collect()
    };
    let key_b: Vec<Value> = {
        let bf = b.fields.lock();
        fields.iter().filter(|f| f.compare).filter_map(|f| bf.get(&f.name).cloned()).collect()
    };
    // Lexicographic compare using values_equal / dispatch_lt for elements.
    let mut less = false;
    let mut equal = true;
    for (va, vb) in key_a.iter().zip(key_b.iter()) {
        if crate::eval::operations::values_equal_pub(va, vb) {
            continue;
        }
        equal = false;
        match crate::types::dispatch_lt(va, vb) {
            Ok(true) => {
                less = true;
                break;
            }
            Ok(false) => {
                less = false;
                break;
            }
            Err(e) => return Some(Err(e)),
        }
    }
    if equal && key_a.len() != key_b.len() {
        equal = false;
        less = key_a.len() < key_b.len();
    }
    let result = match op {
        CmpOp::Eq => equal,
        CmpOp::NotEq => !equal,
        CmpOp::Lt => less && !equal,
        CmpOp::LtE => less || equal,
        CmpOp::Gt => !less && !equal,
        CmpOp::GtE => !less || equal,
        _ => return None,
    };
    Some(Ok(result))
}

/// Derive a `@functools.total_ordering` ordering result. Returns `None` when
/// `op` is not an ordering comparison or `left` is not a `total_ordering`
/// instance; otherwise evaluates the ordering operator the class DOES define
/// plus `__eq__`, and combines them per CPython's `functools` derivations.
fn total_ordering_derive<'a>(
    state: &'a mut InterpreterState,
    op: rustpython_parser::ast::CmpOp,
    left: &'a Value,
    right: &'a Value,
    tools: &'a Tools,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<bool>, EvalError>> + Send + 'a>>
{
    use rustpython_parser::ast::CmpOp;
    Box::pin(async move {
        if !matches!(op, CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE) {
            return Ok(None);
        }
        let Value::Instance(inst) = left else { return Ok(None) };
        let Some(class) = state.classes.get(&inst.class_name) else { return Ok(None) };
        if !class.total_ordering {
            return Ok(None);
        }
        // Pick the ordering root the class defines (in CPython's precedence).
        let root = [
            ("__lt__", CmpOp::Lt),
            ("__le__", CmpOp::LtE),
            ("__gt__", CmpOp::Gt),
            ("__ge__", CmpOp::GtE),
        ]
        .into_iter()
        .find(|(m, _)| class.methods.contains_key(*m));
        let Some((_, root_op)) = root else { return Ok(None) };

        // Evaluate the defined root and `==` through the normal slot dispatch
        // (recursion terminates: both ops resolve a real slot, not this path).
        let (r, _, _) = compare(state, root_op, left, right, tools).await?;
        let (e, _, _) = compare(state, CmpOp::Eq, left, right, tools).await?;

        // self OP other, expressed via r = (self root other) and e = (self == other).
        let result = match (root_op, op) {
            (CmpOp::Lt, CmpOp::LtE) => r || e,
            (CmpOp::Lt, CmpOp::Gt) => !r && !e,
            (CmpOp::Lt, CmpOp::GtE) => !r,
            (CmpOp::LtE, CmpOp::Lt) => r && !e,
            (CmpOp::LtE, CmpOp::Gt) => !r,
            (CmpOp::LtE, CmpOp::GtE) => !r || e,
            (CmpOp::Gt, CmpOp::Lt) => !r && !e,
            (CmpOp::Gt, CmpOp::LtE) => !r,
            (CmpOp::Gt, CmpOp::GtE) => r || e,
            (CmpOp::GtE, CmpOp::Lt) => !r,
            (CmpOp::GtE, CmpOp::LtE) => !r || e,
            (CmpOp::GtE, CmpOp::Gt) => r && !e,
            // Requested op equals the defined root — the direct slot should have
            // handled it; fall through rather than derive.
            _ => return Ok(None),
        };
        Ok(Some(result))
    })
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
    use rustpython_parser::ast::CmpOp;
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
    if let Some(r) = dataclass_order_compare(state, CmpOp::Lt, left, right) {
        return r;
    }
    if let Some(r) =
        crate::eval::modules::functools::try_cmp_key_lt(state, left, right, tools).await
    {
        return r;
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
    // A frozen dataclass is hashable even without a user `__hash__`: CPython
    // auto-generates one over the compare-field tuple for `frozen=True, eq=True`.
    if let Value::Instance(inst) = value {
        if let Some(class) = state.classes.get(&inst.class_name) {
            if class.frozen {
                if let Some(dc_fields) = class.dataclass_fields.clone() {
                    let values: Vec<Value> = {
                        let fields = inst.fields.lock();
                        dc_fields
                            .iter()
                            .filter(|f| f.compare)
                            .map(|f| fields.get(&f.name).cloned().unwrap_or(Value::None))
                            .collect()
                    };
                    return crate::types::dispatch_hash(state, &Value::Tuple(values));
                }
            }
        }
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
    map: &crate::value::SharedDict,
    needle: &Value,
    tools: &Tools,
) -> Result<Option<Value>, EvalError> {
    let h = hash(state, needle, tools).await?;
    // Snapshot the entries so the dict lock isn't held across the async
    // `eq` (which may itself lock / mutate dicts).
    let snapshot = map.lock().clone();
    for (k, v) in &snapshot {
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

/// Dispatch a no-argument unary dunder (`__abs__`, `__int__`, `__index__`, …)
/// on a user-class instance. Returns `None` when `value` is not an instance or
/// its class does not define `slot`, so the caller keeps its builtin handling.
pub async fn instance_unary_dunder(
    state: &mut InterpreterState,
    value: &Value,
    slot: &str,
    tools: &Tools,
) -> Option<Result<Value, EvalError>> {
    let method = instance_slot(state, value, slot)?;
    Some(invoke_slot(state, value, &method, &[], tools).await.map(|(returned, _self)| returned))
}

/// `round(value[, ndigits])` on a user-class instance: dispatches `__round__`,
/// passing `ndigits` only when the caller supplied it (CPython calls
/// `__round__()` with no args for a bare `round(x)`). Returns `None` when the
/// value is not an instance with a `__round__` slot.
pub async fn instance_round_dunder(
    state: &mut InterpreterState,
    value: &Value,
    ndigits: Option<&Value>,
    tools: &Tools,
) -> Option<Result<Value, EvalError>> {
    let method = instance_slot(state, value, "__round__")?;
    let args: &[Value] = match ndigits {
        Some(n) => std::slice::from_ref(n),
        None => &[],
    };
    Some(invoke_slot(state, value, &method, args, tools).await.map(|(returned, _self)| returned))
}

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
    // `len(Color)` on an enum *class* is its member count (an enum class is
    // iterable, so `len` mirrors that), matching CPython's EnumMeta.__len__.
    if let Value::Class(class_name) = value {
        if let Some(class) = state.classes.get(class_name) {
            if class.enum_kind.is_some() {
                return Ok(class.enum_members.len());
            }
        }
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
    // `member in EnumClass` (CPython 3.12 EnumMeta.__contains__): true iff the
    // item is a member of this enum; any non-member is False (not an error).
    if let Value::Class(class_name) = container {
        if state.classes.get(class_name).is_some_and(|c| c.enum_kind.is_some()) {
            return Ok(matches!(item, Value::EnumMember { class_name: m, .. } if m == class_name));
        }
    }
    // An instance without `__contains__` falls back to iterating the container
    // (via `__iter__`, or the `__getitem__` sequence protocol) and comparing
    // each element — CPython's default membership test.
    if matches!(container, Value::Instance(_)) {
        let items = crate::eval::op::iter(state, container, tools).await?;
        for stored in &items {
            if eq(state, stored, item, tools).await? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    if matches!(item, Value::Instance(_)) {
        if let Some(map) = container.as_dict() {
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
        if let Value::Tuple(items) = container {
            for stored in items {
                if eq(state, stored, item, tools).await? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        if let Some(items) = container.set_items() {
            // `x in set` hashes `x` to probe the table, so an unhashable instance
            // (`__hash__ = None`, `__eq__` without `__hash__`) raises TypeError —
            // unlike `x in list`, which only scans. (Membership uses structural
            // eq below, but the hash gate still matches CPython.)
            hash(state, item, tools).await?;
            for stored in &items {
                if eq(state, stored, item, tools).await? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
    }
    // A generator / lazy iterator is consumed by a membership test, comparing
    // each yielded item until a match (`9 in squares(5)`).
    if matches!(container, Value::Generator { .. } | Value::Lazy { .. }) {
        let items = crate::eval::op::iter(state, container, tools).await?;
        for stored in &items {
            if eq(state, stored, item, tools).await? {
                return Ok(true);
            }
        }
        return Ok(false);
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
