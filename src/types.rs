// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Type-object dispatch for built-in values.
//!
//! Each Python operator has one entry point per builtin type — the
//! "slot" — collected on a `&'static TypeObject` table indexed by the
//! `Value` discriminant. `dispatch_*` functions consult the slot, try
//! the reflected slot on `NotImplemented`, and raise CPython-shaped
//! `TypeError` when both sides decline.
//!
//! Design points:
//!
//! * The `Value` enum is the storage shape. Slots receive `&Value` references; they neither own
//!   type identity nor allocate per call.
//! * Slot fns return `Option<bool>` (or `Option<Result<_>>` for the arithmetic family) where `None`
//!   mirrors CPython's `NotImplemented`: the operator should try the other operand's slot before
//!   raising. The value system has no `NotImplemented` singleton; the `Option` sentinel stays
//!   internal to dispatch.
//! * **User-class instances (`Value::Instance`) do NOT go through this table.** The async
//!   eval-layer entry points (`eval_binop`, `eval_compare`, `eval_for`, `eval_subscript`, …) look
//!   up the dunder slot on the class registry first and call into the method body via
//!   `call_method`. This slot table is reached only when the lhs / container is a builtin
//!   primitive.

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    value::Value,
};

/// A built-in type's dispatch entry.
///
/// Each entry carries the type's display name plus per-protocol slots.
/// Adding a new operator slot (matrix multiplication, an in-place op,
/// a new ordering protocol) is additive: extend `TypeObject`, fill the
/// slot on the types that handle it, and the existing call sites don't
/// move. User-class instances bypass this table — they dispatch via the
/// dunder-slot lookup on the class registry.
pub struct TypeObject {
    pub name: &'static str,
    /// Equality slot. Returns `Some(true|false)` if this type handles the
    /// pair; `None` to signal `NotImplemented` (caller should try the
    /// right-hand-side's slot, then fall back to identity / `False`).
    pub eq_slot: EqSlot,
    /// Hash slot. `Some(fn)` for hashable types; `None` for types where
    /// `hash(value)` should raise `TypeError("unhashable type: '<name>'")`.
    /// Slot fns mirror CPython's `_Py_HashDouble` / `long_hash` etc.
    /// line-by-line; see the per-builtin impls below.
    pub hash_slot: Option<HashSlot>,
    /// Less-than slot. Same `Option<bool>` protocol as `eq_slot`: `Some(b)`
    /// means this type handled the pair; `None` means try the other side's
    /// slot, then raise `TypeError`. The slot is expected to handle every
    /// cross-type pair its type knows about — there's no separate `__gt__`
    /// reflected fallback at the builtin level (user classes still pick
    /// up the full rich-compare protocol at the async eval-layer entry).
    pub lt_slot: LtSlot,
    /// Containment slot for `x in container`. `Some(fn)` for iterables /
    /// containers; `None` raises `TypeError("argument of type '<name>' is
    /// not iterable")`. Single-dispatch on the CONTAINER (the right operand
    /// of `in`), unlike eq/lt which are binary.
    pub contains_slot: Option<ContainsSlot>,
    /// Arithmetic slot — handles `+`/`-`/`*`/`/`/`//`/`%`/`**`. ONE slot
    /// per type that dispatches on the `BinOp` tag so adding a new operator
    /// doesn't grow `TypeObject` by another field. Slot returns
    /// `Some(Ok(v))` for "handled, here's the result"; `Some(Err(e))` for
    /// "handled but the op raised (e.g. ZeroDivisionError)"; `None` for
    /// `NotImplemented` — the dispatcher then tries the right-hand-side's
    /// reflected slot before raising `TypeError`.
    pub arith_slot: ArithSlot,
    /// Iteration slot for `for x in iterable`, comprehensions, and the
    /// element-consuming builtins (`sum`/`any`/`all`/`min`/`max`/`sorted`/
    /// `list`/`set`/`tuple`/`zip`). `Some(fn)` for iterables; `None`
    /// raises `TypeError("'<name>' object is not iterable")`. The slot
    /// materializes the iterable into a `Vec<Value>` for now; lazy iter
    /// support (with a proper `Value::Iterator` variant + state) is the
    /// follow-up that lands `iter()`/`next()` builtins.
    pub iter_slot: Option<IterSlot>,
    /// Subscript read slot for `container[key]`. `Some(fn)` for indexable
    /// types; `None` raises `TypeError("'<name>' object is not
    /// subscriptable")`. Slice handling lives in the dispatcher because
    /// slice semantics are uniform across sequence types — only the index
    /// case dispatches per-type.
    pub get_item_slot: Option<GetItemSlot>,
    /// Subscript write slot for `container[key] = value`. `Some(fn)` for
    /// mutable subscriptable types (list, dict); `None` raises
    /// `TypeError("'<name>' object does not support item assignment")`.
    /// Returns the signed byte delta so the caller can update the memory
    /// budget in O(1) without re-estimating the container.
    pub set_item_slot: Option<SetItemSlot>,
    /// Subscript delete slot for `del container[key]`. Same shape as
    /// `set_item_slot`; missing slot raises `TypeError("'<name>' object
    /// does not support item deletion")`.
    pub del_item_slot: Option<DelItemSlot>,
    /// `__missing__` hook for dict-like types. Consulted by the dict
    /// `get_item_slot` on key miss before raising `KeyError`. Counter
    /// uses this to return 0 on a missing count; plain dict leaves
    /// this `None` so a miss still raises `KeyError`.
    pub missing_slot: Option<MissingSlot>,
    /// Length slot for `len(value)`. `Some(fn)` for sized types
    /// (str/bytes/list/tuple/set/dict/range); `None` raises
    /// `TypeError("object of type '<name>' has no len()")`. Truthiness
    /// fallback (`__bool__` -> `__len__() != 0` -> True) consults this
    /// slot indirectly through `dispatch_truthy`.
    pub len_slot: Option<LenSlot>,
    /// Attribute read slot for `obj.name`. `Some(fn)` for types with a
    /// fixed attribute table (dict key-or-method, str/list/set/tuple
    /// method dispatch, exception .message/.args). `None` falls through
    /// to the state-aware path in `eval/names.rs::legacy_attribute`,
    /// which covers Instance/Class/Type/Function/Lambda/Module/Date —
    /// variants whose attribute resolution needs `&InterpreterState`
    /// for class-registry lookups and so cannot live in a `&'static`
    /// slot fn.
    pub get_attr_slot: Option<GetAttrSlot>,
    /// Attribute write slot for `obj.name = value`. `Some(fn)` for the
    /// two builtin types that support attribute writes — Instance and
    /// Dict (which models attribute-as-string-key). Returns the signed
    /// byte delta for the memory budget. `None` raises
    /// `TypeError("'<name>' object has no attribute '<name>' to set")`
    /// via the dispatcher.
    pub set_attr_slot: Option<SetAttrSlot>,
    /// Method-table marker: when true, `method_dispatch` owns a per-type
    /// method table for this builtin (str/list/dict/…). Kept as a bool
    /// rather than an fn pointer so `TypeObject` stays free of the
    /// `eval::functions` dependency cycle; the tables themselves live in
    /// `method_dispatch::METHODS_TABLE` keyed by [`TypeObject::name`].
    pub has_methods_table: bool,
}

/// The seven binary-arithmetic operators dispatched through `arith_slot`.
/// Bitwise ops (`<<`, `>>`, `|`, `^`, `&`) are int-only on builtins so
/// they dispatch directly from `apply_binop` rather than going through
/// the slot table; user-class instances pick up `__lshift__` etc. at
/// the async `eval_binop` entry, before the builtin path is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
}

impl BinOp {
    /// CPython operator symbol for error messages (e.g.
    /// `"unsupported operand type(s) for +: 'list' and 'int'"`).
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::FloorDiv => "//",
            Self::Mod => "%",
            Self::Pow => "**",
        }
    }
}

/// Function-pointer shape for the equality slot. Receives both operands so
/// each type's slot can apply its own coercion / cross-type policy.
pub type EqSlot = fn(lhs: &Value, rhs: &Value) -> Option<bool>;

/// Function-pointer shape for the hash slot. The dispatcher already knows
/// the operand's type matches the slot's owner (it looked up the slot via
/// `type_of`), so the slot only sees its own variant.
pub type HashSlot = fn(value: &Value) -> i64;

/// Function-pointer shape for the less-than slot. Mirrors `EqSlot`.
pub type LtSlot = fn(lhs: &Value, rhs: &Value) -> Option<bool>;

/// Function-pointer shape for the contains slot. Receives the container
/// first, the item being tested second. Container slot impls dispatch back
/// to `dispatch_eq` for element comparisons so bool↔int unification holds
/// inside `in` checks too.
pub type ContainsSlot = fn(container: &Value, item: &Value) -> Result<bool, EvalError>;

/// Function-pointer shape for the arithmetic slot. `None` return signals
/// `NotImplemented` (try the other operand's slot); `Some(Err(...))`
/// reports the operation was handled but raised (e.g. `ZeroDivisionError`).
pub type ArithSlot =
    fn(op: BinOp, lhs: &Value, rhs: &Value, decimal_prec: i64) -> Option<Result<Value, EvalError>>;

/// Function-pointer shape for the iteration slot. Eagerly
/// materializes the iterable into a `Vec<Value>` for the consumer to
/// walk — there is no lazy `Value::Iterator` for builtins, so the
/// memory cost of full materialization is the trade-off for a
/// simpler dispatch model. User-class iterators (which lazily yield
/// via `__next__`) live entirely on the async eval-layer path and
/// don't touch this slot.
pub type IterSlot = fn(value: &Value) -> Result<Vec<Value>, EvalError>;

/// Function-pointer shape for the subscript-read slot.
pub type GetItemSlot = fn(container: &Value, index: &Value) -> Result<Value, EvalError>;

/// Function-pointer shape for the subscript-write slot. Returns the signed
/// byte delta on the container's estimated heap size.
pub type SetItemSlot =
    fn(container: &mut Value, index: &Value, value: Value) -> Result<isize, EvalError>;

/// Function-pointer shape for the subscript-delete slot. Returns the
/// signed byte delta on the container's estimated heap size (always <= 0).
pub type DelItemSlot = fn(container: &mut Value, index: &Value) -> Result<isize, EvalError>;

/// Function-pointer shape for `__missing__`. Called by dict-like
/// `get_item_slot` impls on key miss before raising `KeyError`. Receives
/// the container and the requested key.
pub type MissingSlot = fn(container: &Value, key: &Value) -> Result<Value, EvalError>;

/// Function-pointer shape for the length slot. Returns the count
/// (chars for str, bytes for bytes, items for list/tuple/set, entries
/// for dict, arithmetic for range).
pub type LenSlot = fn(value: &Value) -> Result<usize, EvalError>;

/// Function-pointer shape for the attribute-read slot. State-free —
/// state-dependent variants (Instance/Class) leave this slot `None`
/// and fall through to the state-aware path in `eval/names.rs` which
/// can borrow `&InterpreterState`.
pub type GetAttrSlot = fn(value: &Value, name: &str) -> EvalResult;

/// Function-pointer shape for the attribute-write slot. Mirrors
/// `SetItemSlot`'s contract — returns the signed byte delta the caller
/// folds into the memory budget.
pub type SetAttrSlot =
    fn(value: &mut Value, name: &str, new_val: Value) -> Result<isize, EvalError>;

/// Dispatch `obj.name` through the type-object layer. Returns
/// `Ok(None)` when the value's type has no state-free `get_attr_slot`
/// — the caller is responsible for the state-aware fallback for
/// Instance/Class/Type/Function/Lambda/Module/Date, which need the
/// class registry and so live in `eval/names.rs`.
pub fn dispatch_getattr_opt(value: &Value, name: &str) -> Result<Option<Value>, EvalError> {
    type_of(value).get_attr_slot.map_or_else(|| Ok(None), |slot| slot(value, name).map(Some))
}

/// Dispatch `obj.name = new_val` through the type-object layer.
/// Returns the signed byte delta on the container's estimated heap
/// size. Raises `TypeError` when the value's type has no
/// `set_attr_slot`.
pub fn dispatch_setattr(value: &mut Value, name: &str, new_val: Value) -> Result<isize, EvalError> {
    let type_obj = type_of(value);
    if let Some(slot) = type_obj.set_attr_slot {
        return slot(value, name, new_val);
    }
    Err(InterpreterError::AttributeError(format!(
        "'{}' object has no attribute '{name}'",
        type_obj.name
    ))
    .into())
}

/// Dispatch `container[index]` through the type-object layer. Raises
/// `TypeError("'<name>' object is not subscriptable")` for types without
/// a `get_item_slot`. Slice handling is in the dispatcher's caller; the
/// slot signature only takes a single index value.
pub fn dispatch_getitem(container: &Value, index: &Value) -> Result<Value, EvalError> {
    let container_type = type_of(container);
    container_type.get_item_slot.map_or_else(
        || {
            Err(InterpreterError::TypeError(format!(
                "'{}' object is not subscriptable",
                container_type.name
            ))
            .into())
        },
        |slot| slot(container, index),
    )
}

/// Dispatch `container[index] = value` through the type-object layer.
/// Returns the signed byte delta so memory accounting stays O(1).
pub fn dispatch_setitem(
    container: &mut Value,
    index: &Value,
    value: Value,
) -> Result<isize, EvalError> {
    let container_type = type_of(container);
    if let Some(slot) = container_type.set_item_slot {
        return slot(container, index, value);
    }
    Err(InterpreterError::TypeError(format!(
        "'{}' object does not support item assignment",
        container_type.name
    ))
    .into())
}

/// Dispatch `del container[index]` through the type-object layer. Returns
/// the signed byte delta (<= 0) so the caller can release memory in O(1).
pub fn dispatch_delitem(container: &mut Value, index: &Value) -> Result<isize, EvalError> {
    let container_type = type_of(container);
    if let Some(slot) = container_type.del_item_slot {
        return slot(container, index);
    }
    Err(InterpreterError::TypeError(format!(
        "'{}' object does not support item deletion",
        container_type.name
    ))
    .into())
}

/// Dispatch `len(value)` through the type-object layer. Raises
/// `TypeError("object of type '<name>' has no len()")` for types without
/// a `len_slot`.
pub fn dispatch_len(value: &Value) -> Result<usize, EvalError> {
    let type_obj = type_of(value);
    type_obj.len_slot.map_or_else(
        || {
            Err(InterpreterError::TypeError(format!(
                "object of type '{}' has no len()",
                type_obj.name
            ))
            .into())
        },
        |slot| slot(value),
    )
}

/// Dispatch iteration through the type-object layer. Returns the
/// materialized `Vec<Value>` for the consumer to walk; raises
/// `TypeError("'<name>' object is not iterable")` when the type's
/// `iter_slot` is `None`.
pub fn dispatch_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    let type_obj = type_of(value);
    type_obj.iter_slot.map_or_else(
        || {
            Err(InterpreterError::TypeError(format!("'{}' object is not iterable", type_obj.name))
                .into())
        },
        |slot| slot(value),
    )
}

/// Dispatch a binary arithmetic op (`+`/`-`/`*`/`/`/`//`/`%`/`**`) through
/// the type-object layer. Tries `lhs`'s `arith_slot`; on `NotImplemented`
/// tries `rhs`'s `arith_slot` for the reflected dunder path; on a second
/// `NotImplemented` raises `TypeError` with CPython's exact wording
/// ("unsupported operand type(s) for <op>: 'X' and 'Y'").
pub fn dispatch_binop(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    decimal_prec: i64,
) -> Result<Value, EvalError> {
    // IntEnum / StrEnum members unwrap to their underlying value for
    // arithmetic — they're functionally `int` / `str` subclasses in
    // CPython. Plain Enum keeps its type so the dispatcher raises
    // TypeError, matching CPython's "unsupported operand type" wording.
    let lhs_u = unwrap_enum_for_compare(lhs);
    let rhs_u = unwrap_enum_for_compare(rhs);
    if !std::ptr::eq(lhs_u, lhs) || !std::ptr::eq(rhs_u, rhs) {
        return dispatch_binop(op, lhs_u, rhs_u, decimal_prec);
    }
    let lhs_type = type_of(lhs);
    if let Some(result) = (lhs_type.arith_slot)(op, lhs, rhs, decimal_prec) {
        return result;
    }
    let rhs_type = type_of(rhs);
    if let Some(result) = (rhs_type.arith_slot)(op, lhs, rhs, decimal_prec) {
        return result;
    }
    Err(InterpreterError::TypeError(format!(
        "unsupported operand type(s) for {}: '{}' and '{}'",
        op.symbol(),
        lhs_type.name,
        rhs_type.name,
    ))
    .into())
}

/// Dispatch `lhs < rhs` through the type-object layer. Both operand slots
/// get a chance to handle the pair; double-`NotImplemented` raises
/// `TypeError("'<' not supported between instances of 'X' and 'Y'")`,
/// matching CPython's wording so error-shape tests don't drift.
pub fn dispatch_lt(lhs: &Value, rhs: &Value) -> Result<bool, EvalError> {
    // IntEnum / StrEnum members unwrap for comparison (same shape as
    // `dispatch_binop`). Plain Enum keeps its type and the dispatcher
    // raises TypeError per CPython.
    let lhs_u = unwrap_enum_for_compare(lhs);
    let rhs_u = unwrap_enum_for_compare(rhs);
    if !std::ptr::eq(lhs_u, lhs) || !std::ptr::eq(rhs_u, rhs) {
        return dispatch_lt(lhs_u, rhs_u);
    }
    let lhs_type = type_of(lhs);
    if let Some(result) = (lhs_type.lt_slot)(lhs, rhs) {
        return Ok(result);
    }
    let rhs_type = type_of(rhs);
    if let Some(result) = (rhs_type.lt_slot)(lhs, rhs) {
        return Ok(result);
    }
    Err(type_error_unsupported("<", lhs, rhs))
}

/// Unwrap an EnumMember to its underlying value when its kind is
/// Int or Str. Plain Enum members are returned as-is.
fn unwrap_enum_for_compare(value: &Value) -> &Value {
    match value {
        Value::EnumMember {
            value: inner,
            kind: crate::value::EnumKind::Int | crate::value::EnumKind::Str,
            ..
        } => inner.as_ref(),
        _ => value,
    }
}

/// Dispatch `item in container` through the container's `contains_slot`.
/// Returns `TypeError("argument of type '<name>' is not iterable")` when
/// the container type has no slot — matches CPython's error surface for
/// `1 in 2`.
pub fn dispatch_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let container_type = type_of(container);
    container_type.contains_slot.map_or_else(
        || {
            Err(InterpreterError::TypeError(format!(
                "argument of type '{}' is not iterable",
                container_type.name
            ))
            .into())
        },
        |slot| slot(container, item),
    )
}

/// Dispatch `hash(value)` through the type-object layer. Returns
/// `TypeError("unhashable type: '<name>'")` when the type's `hash_slot` is
/// `None` — matches CPython's error surface for `hash([])`, `hash({})`, etc.
///
/// Instance values consult the class registry: a `@dataclass`-decorated
/// class with default kwargs (eq=True, frozen=False) is unhashable per
/// CPython, which explicitly sets `__hash__ = None`. Regular user classes
/// fall through to the standard identity-shaped hash on the type's slot.
pub fn dispatch_hash(state: &InterpreterState, value: &Value) -> Result<i64, EvalError> {
    if let Value::Instance(inst) = value {
        if let Some(class) = state.classes.get(&inst.class_name) {
            // Default `@dataclass` (eq=True, frozen=False) sets
            // `__hash__ = None`, so the instance is unhashable. A
            // user-defined `__hash__` is honoured at the async eval
            // layer only — the sync `dispatch_hash` here doesn't
            // call it, so user classes overriding `__hash__` reach
            // the catch-all DefaultHasher route below.
            if class.dataclass_fields.is_some() && !class.methods.contains_key("__hash__") {
                return Err(InterpreterError::TypeError(format!(
                    "unhashable type: '{}'",
                    inst.class_name
                ))
                .into());
            }
        }
    }
    let type_obj = type_of(value);
    type_obj.hash_slot.map_or_else(
        || Err(InterpreterError::TypeError(format!("unhashable type: '{}'", type_obj.name)).into()),
        |slot| Ok(slot(value)),
    )
}

/// Dispatch `lhs == rhs` through the type-object layer.
///
/// Tries the left type's slot first; on `NotImplemented` tries the right
/// type's slot; on a second `NotImplemented` returns `Ok(false)` per
/// CPython's "objects of different types compare unequal" default. User-
/// class instances route through their `__eq__` method if defined.
pub fn dispatch_eq(state: &InterpreterState, lhs: &Value, rhs: &Value) -> EvalResult {
    // User-class instance eq: look up `__eq__` on the class registry, fall
    // back to identity comparison if undefined. Direct shortcut here so we
    // don't have to thread `state` through every builtin slot fn.
    if let Value::Instance(inst) = lhs {
        // `@dataclass`-synthesized __eq__: when both sides are instances of
        // the same dataclass class, compare the field-tuple under each
        // field's `compare` flag. Matches CPython, which produces an
        // `__eq__` that runs `self.<fields> == other.<fields>`.
        if let Value::Instance(other_inst) = rhs {
            if inst.class_name == other_inst.class_name {
                if let Some(class) = state.classes.get(&inst.class_name) {
                    if let Some(fields) = &class.dataclass_fields {
                        if !class.methods.contains_key("__eq__") {
                            let mut equal = true;
                            let af = inst.fields.lock();
                            let bf = other_inst.fields.lock();
                            for field in fields.iter().filter(|f| f.compare) {
                                match (af.get(&field.name), bf.get(&field.name)) {
                                    (Some(a), Some(b)) => {
                                        let cmp = dispatch_eq(state, a, b)?;
                                        if !matches!(cmp, Value::Bool(true)) {
                                            equal = false;
                                            break;
                                        }
                                    }
                                    (None, None) => {}
                                    _ => {
                                        equal = false;
                                        break;
                                    }
                                }
                            }
                            return Ok(Value::Bool(equal));
                        }
                    }
                }
            }
        }
        // User-defined `__eq__` is dispatched at the async eval-layer
        // entry (`eval_compare`), not here. Sync `dispatch_eq` is
        // reached only after the async path declined to short-circuit
        // — at which point the class has no `__eq__` or we're in a
        // context where method dispatch isn't possible (hash, set
        // membership). Identity fallback matches CPython's default.
        return Ok(Value::Bool(
            matches!(rhs, Value::Instance(other_inst) if std::ptr::eq(inst, other_inst)),
        ));
    }
    let lhs_type = type_of(lhs);
    if let Some(result) = (lhs_type.eq_slot)(lhs, rhs) {
        return Ok(Value::Bool(result));
    }
    let rhs_type = type_of(rhs);
    if let Some(result) = (rhs_type.eq_slot)(rhs, lhs) {
        return Ok(Value::Bool(result));
    }
    // Both slots returned NotImplemented; CPython treats this as False
    // ("objects of different types compare unequal") rather than raising.
    Ok(Value::Bool(false))
}

/// Map a `Value` variant to its `TypeObject`. Static dispatch by tag —
/// O(1) and inlines well. Unmigrated variants fall through to
/// `OBJECT_TYPE`'s catch-all slot impls.
fn type_of(value: &Value) -> &'static TypeObject {
    match value {
        Value::None => &NONE_TYPE,
        Value::Bool(_) => &BOOL_TYPE,
        Value::Int(_) | Value::BigInt(_) => &INT_TYPE,
        Value::Float(_) => &FLOAT_TYPE,
        Value::String(_) => &STR_TYPE,
        Value::Bytes(_) => &BYTES_TYPE,
        Value::List(_) => &LIST_TYPE,
        Value::Tuple(_) => &TUPLE_TYPE,
        Value::Dict(_) => &DICT_TYPE,
        Value::Set(_) => &SET_TYPE,
        Value::Range { .. } => &RANGE_TYPE,
        Value::Counter(_) => &COUNTER_TYPE,
        Value::Deque { .. } => &DEQUE_TYPE,
        Value::DefaultDict { .. } => &DEFAULTDICT_TYPE,
        Value::Decimal(_) => &DECIMAL_TYPE,
        Value::Fraction(_) => &FRACTION_TYPE,
        Value::Date(_) => &DATE_TYPE,
        Value::DateTime { .. } => &DATETIME_TYPE,
        Value::Time(_) => &TIME_TYPE,
        Value::TimeDelta(_) => &TIMEDELTA_TYPE,
        Value::TimeZone(_) => &TIMEZONE_TYPE,
        Value::HashDigest { .. } => &HASHDIGEST_TYPE,
        Value::EnumMember { .. } => &ENUMMEMBER_TYPE,
        // The remaining variants (Function, Lambda, Instance, …) are not
        // yet migrated; their eq falls back to the existing comparator via
        // OBJECT_TYPE's catch-all eq.
        _ => &OBJECT_TYPE,
    }
}

/// Display name of the builtin type object for `value`.
#[must_use]
pub fn type_name_of(value: &Value) -> &'static str {
    type_of(value).name
}

/// Whether this value's type has a per-type method table in `method_dispatch`.
#[must_use]
pub fn type_has_methods_table(value: &Value) -> bool {
    type_of(value).has_methods_table
}

// ---------------------------------------------------------------------------
// Builtin type singletons
// ---------------------------------------------------------------------------

static NONE_TYPE: TypeObject = TypeObject {
    name: "NoneType",
    eq_slot: none_eq,
    hash_slot: Some(none_hash),
    lt_slot: noimpl_lt,
    contains_slot: None,
    arith_slot: noimpl_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: Some(noattr_get_attr),
    set_attr_slot: None,
    has_methods_table: false,
};
static BOOL_TYPE: TypeObject = TypeObject {
    name: "bool",
    eq_slot: bool_eq,
    hash_slot: Some(bool_hash),
    lt_slot: bool_lt,
    contains_slot: None,
    arith_slot: numeric_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: Some(noattr_get_attr),
    set_attr_slot: None,
    has_methods_table: false,
};
static INT_TYPE: TypeObject = TypeObject {
    name: "int",
    eq_slot: int_eq,
    hash_slot: Some(int_hash_slot),
    lt_slot: int_lt,
    contains_slot: None,
    arith_slot: numeric_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: Some(noattr_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
static FLOAT_TYPE: TypeObject = TypeObject {
    name: "float",
    eq_slot: float_eq,
    hash_slot: Some(float_hash_slot),
    lt_slot: float_lt,
    contains_slot: None,
    arith_slot: numeric_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: Some(noattr_get_attr),
    set_attr_slot: None,
    has_methods_table: false,
};
static STR_TYPE: TypeObject = TypeObject {
    name: "str",
    eq_slot: str_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: str_lt,
    contains_slot: Some(str_contains),
    arith_slot: str_arith,
    iter_slot: Some(str_iter),
    get_item_slot: Some(str_get_item),
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: Some(str_len),
    get_attr_slot: Some(str_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
static BYTES_TYPE: TypeObject = TypeObject {
    name: "bytes",
    eq_slot: bytes_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: bytes_lt,
    contains_slot: None,
    arith_slot: bytes_arith,
    iter_slot: Some(bytes_iter),
    get_item_slot: Some(bytes_get_item),
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: Some(bytes_len),
    get_attr_slot: Some(noattr_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
static LIST_TYPE: TypeObject = TypeObject {
    name: "list",
    eq_slot: list_eq,
    hash_slot: None,
    lt_slot: list_lt,
    contains_slot: Some(sequence_contains),
    arith_slot: list_arith,
    iter_slot: Some(sequence_iter),
    get_item_slot: Some(sequence_get_item),
    set_item_slot: Some(list_set_item),
    del_item_slot: Some(list_del_item),
    missing_slot: None,
    len_slot: Some(sequence_len),
    get_attr_slot: Some(list_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
static TUPLE_TYPE: TypeObject = TypeObject {
    name: "tuple",
    eq_slot: tuple_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: tuple_lt,
    contains_slot: Some(sequence_contains),
    arith_slot: tuple_arith,
    iter_slot: Some(sequence_iter),
    get_item_slot: Some(sequence_get_item),
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: Some(sequence_len),
    get_attr_slot: Some(tuple_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
static DICT_TYPE: TypeObject = TypeObject {
    name: "dict",
    eq_slot: dict_eq,
    hash_slot: None,
    lt_slot: noimpl_lt,
    contains_slot: Some(dict_contains),
    arith_slot: noimpl_arith,
    // Iterating a dict yields its keys, matching CPython.
    iter_slot: Some(dict_iter),
    get_item_slot: Some(dict_get_item),
    set_item_slot: Some(dict_set_item),
    del_item_slot: Some(dict_del_item),
    // Plain dict has no __missing__; Counter (B3) will set this slot on
    // its own TypeObject so the dict get_item slot picks it up via
    // type_of(container).missing_slot rather than a separate per-key check.
    missing_slot: None,
    len_slot: Some(dict_len),
    get_attr_slot: Some(dict_get_attr),
    // CPython: `d.foo = 1` raises `AttributeError("'dict' object has
    // no attribute 'foo'")`. A6 closes the pre-existing divergence
    // where dict accepted attribute writes as string-key inserts.
    set_attr_slot: None,
    has_methods_table: true,
};
static SET_TYPE: TypeObject = TypeObject {
    name: "set",
    eq_slot: set_eq,
    hash_slot: None,
    lt_slot: noimpl_lt,
    contains_slot: Some(sequence_contains),
    arith_slot: set_arith,
    iter_slot: Some(sequence_iter),
    // Sets are not subscriptable in CPython (set has no __getitem__).
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: Some(sequence_len),
    get_attr_slot: Some(set_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
/// Range first-class TypeObject. Supports iteration, membership
/// (with the step-aware modular check), `len`, and indexed access —
/// no arithmetic, no ordering.
static RANGE_TYPE: TypeObject = TypeObject {
    name: "range",
    eq_slot: object_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: noimpl_lt,
    contains_slot: Some(range_contains),
    arith_slot: noimpl_arith,
    iter_slot: Some(range_iter),
    get_item_slot: Some(range_get_item),
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: Some(range_len),
    get_attr_slot: Some(noattr_get_attr),
    set_attr_slot: None,
    has_methods_table: false,
};
/// `collections.Counter` first-class TypeObject. Inherits dict's slot
/// shape — same get_item / set_item / del_item / iter / contains /
/// len — but its `missing_slot` returns `Int(0)` (without inserting,
/// matching CPython's `__missing__` semantics on dict subclasses).
/// Multiset arithmetic (+/-/&/|) routes through `counter_arith`. The
/// distinct name lets isinstance see Counter as a separate type while
/// `check_isinstance` recognises Counter as a dict subclass via the
/// builtin-MRO table in functions.rs.
static COUNTER_TYPE: TypeObject = TypeObject {
    name: "Counter",
    eq_slot: counter_eq,
    hash_slot: None,
    lt_slot: noimpl_lt,
    contains_slot: Some(counter_contains),
    arith_slot: counter_arith,
    iter_slot: Some(counter_iter),
    get_item_slot: Some(counter_get_item),
    set_item_slot: Some(counter_set_item),
    del_item_slot: Some(counter_del_item),
    // The defining feature: missing keys return Int(0) without
    // inserting — matches CPython's `Counter.__missing__`.
    missing_slot: Some(counter_missing),
    len_slot: Some(counter_len),
    get_attr_slot: Some(counter_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
/// `collections.deque` TypeObject. Iteration, containment,
/// length, and indexing inherit from VecDeque. Arithmetic /
/// assignment / deletion all go through deque-specific method
/// dispatch in eval/functions.rs (which carries the &mut backing).
static DEQUE_TYPE: TypeObject = TypeObject {
    name: "deque",
    eq_slot: noimpl_eq,
    hash_slot: None,
    lt_slot: noimpl_lt,
    contains_slot: Some(deque_contains),
    arith_slot: noimpl_arith,
    iter_slot: Some(deque_iter),
    get_item_slot: Some(deque_get_item),
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: Some(deque_len),
    get_attr_slot: Some(deque_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
/// `collections.defaultdict` TypeObject. Inherits dict's
/// slot shape — same get_item / set_item / del_item / iter / contains
/// / len — over the items map. Missing-key synthesis happens in
/// eval_subscript (needs &mut state + async).
static DEFAULTDICT_TYPE: TypeObject = TypeObject {
    name: "defaultdict",
    eq_slot: noimpl_eq,
    hash_slot: None,
    lt_slot: noimpl_lt,
    contains_slot: Some(defaultdict_contains),
    arith_slot: noimpl_arith,
    iter_slot: Some(defaultdict_iter),
    // get_item is intentionally `None` so dispatch_getitem raises;
    // eval_subscript intercepts DefaultDict and runs the factory
    // before reaching dispatch. Reading via .get(key) routes through
    // dict_get_item via the dispatch_dict_method shim.
    get_item_slot: None,
    set_item_slot: Some(defaultdict_set_item),
    del_item_slot: Some(defaultdict_del_item),
    missing_slot: None,
    len_slot: Some(defaultdict_len),
    get_attr_slot: Some(dict_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
/// `hashlib` digest TypeObject. HashDigest's
/// surface is attribute-style methods (`.hexdigest()`, `.digest()`,
/// `.update()`); the slot routes through hashlib's existing
/// `hash_attribute` resolver which returns method-marker sentinels
/// the method dispatcher in functions.rs recognises.
static HASHDIGEST_TYPE: TypeObject = TypeObject {
    name: "_hashlib.HASH",
    eq_slot: object_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: noimpl_lt,
    contains_slot: None,
    arith_slot: noimpl_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: Some(hashdigest_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
/// `enum.Enum` member TypeObject. Exposes `.name`
/// and `.value`. Equality / ordering / arithmetic stay on the existing
/// unwrap-then-recurse helpers in `dispatch_eq` / `dispatch_lt` /
/// `dispatch_binop` since they need to bounce through the underlying
/// type's dispatch — promoting to a slot would just hide the same
/// recursion behind another layer.
static ENUMMEMBER_TYPE: TypeObject = TypeObject {
    name: "enum",
    eq_slot: object_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: noimpl_lt,
    contains_slot: None,
    arith_slot: noimpl_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: Some(enummember_get_attr),
    set_attr_slot: None,
    has_methods_table: false,
};
/// `datetime.date` TypeObject. Arithmetic and
/// attribute access route through the shared datetime cluster slots;
/// equality / ordering fall back to OBJECT_TYPE's identity / noimpl
/// behaviour (preserved across the move).
static DATE_TYPE: TypeObject = TypeObject {
    name: "date",
    eq_slot: object_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: noimpl_lt,
    contains_slot: None,
    arith_slot: datetime_cluster_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: Some(date_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
static DATETIME_TYPE: TypeObject = TypeObject {
    name: "datetime",
    eq_slot: object_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: noimpl_lt,
    contains_slot: None,
    arith_slot: datetime_cluster_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: Some(datetime_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
static TIME_TYPE: TypeObject = TypeObject {
    name: "time",
    eq_slot: object_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: noimpl_lt,
    contains_slot: None,
    arith_slot: noimpl_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: Some(time_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
static TIMEDELTA_TYPE: TypeObject = TypeObject {
    name: "timedelta",
    eq_slot: object_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: noimpl_lt,
    contains_slot: None,
    arith_slot: datetime_cluster_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: Some(timedelta_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
/// `datetime.timezone` TypeObject. No arithmetic, no attribute surface
/// beyond construction; lives here for completeness so `type(tz)`
/// reports `'timezone'` rather than `'object'` once instances reach
/// the dispatch layer.
static TIMEZONE_TYPE: TypeObject = TypeObject {
    name: "timezone",
    eq_slot: object_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: noimpl_lt,
    contains_slot: None,
    arith_slot: noimpl_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: None,
    set_attr_slot: None,
    has_methods_table: false,
};
/// `decimal.Decimal` TypeObject. Arithmetic + ordering + equality
/// (with int-lift on either side) flow through the regular dispatch
/// slots — no intercept hacks in the kernel.
static DECIMAL_TYPE: TypeObject = TypeObject {
    name: "Decimal",
    eq_slot: decimal_eq,
    hash_slot: None,
    lt_slot: decimal_lt,
    contains_slot: None,
    arith_slot: decimal_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: None,
    set_attr_slot: None,
    has_methods_table: false,
};
/// `fractions.Fraction` TypeObject. Same Pass 2a promotion as Decimal,
/// plus a `get_attr_slot` for `.numerator` / `.denominator` — moved out
/// of `eval/names.rs::legacy_attribute`.
static FRACTION_TYPE: TypeObject = TypeObject {
    name: "Fraction",
    eq_slot: fraction_eq,
    hash_slot: None,
    lt_slot: fraction_lt,
    contains_slot: None,
    arith_slot: fraction_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    get_attr_slot: Some(fraction_get_attr),
    set_attr_slot: None,
    has_methods_table: false,
};
static OBJECT_TYPE: TypeObject = TypeObject {
    name: "object",
    eq_slot: object_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: noimpl_lt,
    // Anything not yet promoted (Function/Lambda/Instance/Exception/etc.)
    // still routes contains through the legacy path. The list of variants
    // here shrinks slice-by-slice.
    contains_slot: Some(object_contains),
    arith_slot: noimpl_arith,
    // No catch-all iter_slot: an unmigrated variant should raise the
    // "not iterable" TypeError, matching CPython's surface for things like
    // `for x in 1`.
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    // No catch-all get_attr_slot — eval_attribute falls through to the
    // legacy state-aware path for the variants behind OBJECT_TYPE
    // (Instance/Class/Type/Function/Lambda/Module/Date/Exception). B1's
    // user-class TypeObject promotion replaces that with a state-aware
    // slot impl.
    get_attr_slot: None,
    set_attr_slot: None,
    has_methods_table: false,
};

// ---------------------------------------------------------------------------
// Eq slot implementations — one per builtin
// ---------------------------------------------------------------------------

#[expect(
    clippy::unnecessary_wraps,
    reason = "slot fns return Option<bool> to fit the EqSlot fn-pointer type; None means NotImplemented (try the other operand). Same-type slots always handle, so they always Some(...); breaking the protocol would require a separate slot table per arity."
)]
const fn none_eq(_lhs: &Value, rhs: &Value) -> Option<bool> {
    Some(matches!(rhs, Value::None))
}

fn bool_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::Bool(a) = lhs else { return None };
    match rhs {
        Value::Bool(b) => Some(a == b),
        // Cross-type: `True == 1` and `False == 0` (CPython treats bool as int subclass).
        Value::Int(i) => Some(*i == i64::from(*a)),
        Value::BigInt(i) => Some(i.as_ref() == &num_bigint::BigInt::from(i64::from(*a))),
        Value::Float(f) => Some(*f == if *a { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn int_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let a = crate::value::value_as_bigint(lhs)?;
    match rhs {
        Value::Int(_) | Value::BigInt(_) | Value::Bool(_) => {
            let b = crate::value::value_as_bigint(rhs)?;
            Some(a == b)
        }
        Value::Float(f) => {
            use num_traits::ToPrimitive as _;
            Some(a.to_f64().is_some_and(|af| *f == af))
        }
        _ => None,
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "Python int↔float eq matches CPython's lossy compare"
)]
fn float_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::Float(a) = lhs else { return None };
    match rhs {
        Value::Float(b) => Some(a == b),
        Value::Bool(b) => Some(*a == if *b { 1.0 } else { 0.0 }),
        Value::Int(i) => Some(*a == (*i as f64)),
        _ => None,
    }
}

fn str_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::String(a) = lhs else { return None };
    let Value::String(b) = rhs else { return None };
    Some(a == b)
}

fn bytes_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::Bytes(a) = lhs else { return None };
    let Value::Bytes(b) = rhs else { return None };
    Some(a == b)
}

fn list_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::List(a) = lhs else { return None };
    let Value::List(b) = rhs else { return None };
    if std::sync::Arc::ptr_eq(a, b) {
        return Some(true);
    }
    let a_guard = a.lock();
    let b_guard = b.lock();
    Some(elementwise_eq(&a_guard, &b_guard))
}

fn tuple_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::Tuple(a) = lhs else { return None };
    let Value::Tuple(b) = rhs else { return None };
    Some(elementwise_eq(a, b))
}

fn dict_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::Dict(a) = lhs else { return None };
    let Value::Dict(b) = rhs else { return None };
    if a.len() != b.len() {
        return Some(false);
    }
    let equal = a.iter().all(|(k, v)| b.get(k).is_some_and(|bv| recurse_eq(v, bv)));
    Some(equal)
}

fn set_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::Set(a) = lhs else { return None };
    let Value::Set(b) = rhs else { return None };
    if a.len() != b.len() {
        return Some(false);
    }
    // Set equality is unordered: every element in a must appear in b under
    // the same eq semantics.
    let equal = a.iter().all(|av| b.iter().any(|bv| recurse_eq(av, bv)));
    Some(equal)
}

/// Catch-all eq for variants without their own per-type slot (Function,
/// Lambda, Exception, LazyProxy, Module, ModuleFunction, ReMatch, …).
/// Routes through the shared `values_equal` comparator so all the
/// historical fall-through behaviour is preserved in one place rather
/// than re-implementing it per variant.
#[expect(
    clippy::unnecessary_wraps,
    reason = "EqSlot fn-pointer protocol requires Option<bool>; object_eq always handles via the shared comparator so Some(...) is correct"
)]
fn object_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    Some(crate::eval::operations::values_equal_pub(lhs, rhs))
}

fn elementwise_eq(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| recurse_eq(x, y))
}

/// Recurse into the type dispatch for an inner-element compare. Falls back
/// to `false` when both sides return `NotImplemented`.
fn recurse_eq(lhs: &Value, rhs: &Value) -> bool {
    let lhs_type = type_of(lhs);
    if let Some(result) = (lhs_type.eq_slot)(lhs, rhs) {
        return result;
    }
    let rhs_type = type_of(rhs);
    if let Some(result) = (rhs_type.eq_slot)(rhs, lhs) {
        return result;
    }
    false
}

// ---------------------------------------------------------------------------
// Less-than slot implementations
// ---------------------------------------------------------------------------

/// Catch-all `lt_slot` for types without a defined ordering (None, dict,
/// set, plus `OBJECT_TYPE` for the unmigrated variants). Always returns
/// `None` so `dispatch_lt` raises `TypeError` via the unsupported-pair
/// fallback.
const fn noimpl_lt(_lhs: &Value, _rhs: &Value) -> Option<bool> {
    None
}

fn bool_lt(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::Bool(a) = lhs else { return None };
    let av = i64::from(*a);
    match rhs {
        Value::Bool(b) => Some(av < i64::from(*b)),
        Value::Int(b) => Some(av < *b),
        #[expect(
            clippy::cast_precision_loss,
            reason = "Python bool↔float compare matches CPython's lossy compare"
        )]
        Value::Float(b) => Some((av as f64) < *b),
        _ => None,
    }
}

fn int_lt(lhs: &Value, rhs: &Value) -> Option<bool> {
    let a = crate::value::value_as_bigint(lhs)?;
    match rhs {
        Value::Int(_) | Value::BigInt(_) | Value::Bool(_) => {
            let b = crate::value::value_as_bigint(rhs)?;
            Some(a < b)
        }
        Value::Float(b) => {
            use num_traits::ToPrimitive as _;
            Some(a.to_f64().is_some_and(|af| af < *b))
        }
        _ => None,
    }
}

fn float_lt(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::Float(a) = lhs else { return None };
    match rhs {
        Value::Float(b) => Some(a < b),
        Value::Bool(b) => Some(*a < if *b { 1.0 } else { 0.0 }),
        #[expect(
            clippy::cast_precision_loss,
            reason = "Python int↔float compare matches CPython's lossy compare"
        )]
        Value::Int(b) => Some(*a < (*b as f64)),
        Value::BigInt(b) => {
            use num_traits::ToPrimitive as _;
            Some(b.to_f64().is_some_and(|bf| *a < bf))
        }
        _ => None,
    }
}

fn str_lt(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::String(a) = lhs else { return None };
    let Value::String(b) = rhs else { return None };
    Some(a < b)
}

fn bytes_lt(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::Bytes(a) = lhs else { return None };
    let Value::Bytes(b) = rhs else { return None };
    Some(a < b)
}

fn list_lt(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::List(a) = lhs else { return None };
    let Value::List(b) = rhs else { return None };
    let a_guard = a.lock();
    let b_guard = b.lock();
    Some(lex_lt(&a_guard, &b_guard))
}

fn tuple_lt(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::Tuple(a) = lhs else { return None };
    let Value::Tuple(b) = rhs else { return None };
    Some(lex_lt(a, b))
}

/// Lexicographic less-than for list/tuple: first non-equal element decides;
/// if one is a prefix of the other, the shorter is less. Equality recurses
/// through the eq dispatch (so bool↔int unification holds inside lists too).
fn lex_lt(a: &[Value], b: &[Value]) -> bool {
    for (x, y) in a.iter().zip(b.iter()) {
        if !recurse_eq(x, y) {
            // First inequal position decides. Nested user-class
            // ordering is best-effort here (sync slot table can't reach
            // `__lt__` on an Instance); fall back to `false` if the
            // sync dispatch declines.
            return dispatch_lt(x, y).unwrap_or(false);
        }
    }
    a.len() < b.len()
}

// ---------------------------------------------------------------------------
// Contains slot implementations
// ---------------------------------------------------------------------------

/// `x in list`/`x in tuple`/`x in set`: element-wise equality scan via the
/// eq dispatch (so `True in [1]` is `True` per CPython's bool↔int rule).
#[expect(
    clippy::unnecessary_wraps,
    reason = "ContainsSlot protocol fixes the Result<bool, EvalError> signature; slots that can't error still keep it so call sites stay homogeneous across all container types"
)]
fn sequence_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    // List is shared via Arc<Mutex<Vec>>, so it locks for the scan;
    // Tuple/Set still wrap a plain Vec and borrow directly. The
    // contains scan never recurses through interpreter eval, so the
    // lock guard's scope is bounded by this loop.
    if let Value::List(items) = container {
        let snapshot = items.lock().clone();
        for entry in &snapshot {
            if recurse_eq(item, entry) {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    let (Value::Tuple(items) | Value::Set(items)) = container else {
        unreachable!("sequence_contains only attached to list/tuple/set TypeObjects")
    };
    for entry in items {
        if recurse_eq(item, entry) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `key in dict`: hash-based lookup against the dict's keys.
#[expect(
    clippy::unnecessary_wraps,
    reason = "ContainsSlot protocol fixes the Result signature; see sequence_contains rationale"
)]
fn dict_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let Value::Dict(map) = container else {
        unreachable!("dict_contains only attached to DICT_TYPE")
    };
    let Ok(key) = crate::eval::literals::value_to_key(item) else {
        return Ok(false);
    };
    Ok(map.contains_key(&key))
}

/// `needle in str`: substring check. CPython requires `needle` to be a str
/// — `1 in "abc"` raises `TypeError`.
fn str_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let Value::String(s) = container else { unreachable!("str_contains only on STR_TYPE") };
    let Value::String(needle) = item else {
        return Err(InterpreterError::TypeError(format!(
            "'in <string>' requires string as left operand, not '{}'",
            item.type_name()
        ))
        .into());
    };
    Ok(s.contains(needle.as_str()))
}

/// Catch-all contains for non-iterable variants (Function, Lambda,
/// Exception, …). Always raises CPython's "argument of type '<name>'
/// is not iterable" TypeError. User-class instances with
/// `__contains__` are intercepted at the async eval-layer entry, so
/// they never reach this catch-all.
fn object_contains(container: &Value, _item: &Value) -> Result<bool, EvalError> {
    Err(InterpreterError::TypeError(format!(
        "argument of type '{}' is not iterable",
        container.type_name(),
    ))
    .into())
}

// ---------------------------------------------------------------------------
// Arithmetic slot implementations
// ---------------------------------------------------------------------------

/// Catch-all `arith_slot` for types that don't participate in any of the
/// seven arithmetic operators (None, bytes, dict, plus `OBJECT_TYPE`'s
/// catch-all for unmigrated variants). Always returns `None` so
/// `dispatch_binop` raises the unsupported-pair `TypeError`.
const fn noimpl_arith(
    _op: BinOp,
    _lhs: &Value,
    _rhs: &Value,
    _decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    None
}

/// Numeric arithmetic slot shared by bool/int/float. Every cross-type
/// numeric pair is handled here regardless of which side's slot was hit
/// first — bool↔int↔float coercion uses CPython's "promote to widest type"
/// rule (any float operand → float result; otherwise int). String/list/
/// tuple repetition (`5 * "abc"`) is also handled here on the int side
/// since the rhs's str/list/tuple `arith_slot` handles the symmetric case.
fn numeric_arith(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    _decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    if !is_numeric(lhs) {
        return None;
    }
    if is_numeric(rhs) {
        return Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs));
    }
    // Int * str / Int * list / Int * tuple (repetition) — delegate to the
    // legacy mul path which handles both orderings.
    if matches!(op, BinOp::Mul)
        && matches!(rhs, Value::String(_) | Value::List(_) | Value::Tuple(_))
    {
        return Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs));
    }
    None
}

/// `str + str` (concat), `str * int` (repetition), and `str % args`
/// (printf-style formatting). Other ops surface as `NotImplemented`.
fn str_arith(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    _decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    let Value::String(_) = lhs else { return None };
    match op {
        BinOp::Add if matches!(rhs, Value::String(_)) => {
            Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs))
        }
        BinOp::Mul if matches!(rhs, Value::Int(_) | Value::Bool(_)) => {
            Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs))
        }
        // `str % args` is printf-style formatting. The args can be a tuple,
        // a dict, or a single value — the legacy mod_values dispatches.
        BinOp::Mod => Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs)),
        _ => None,
    }
}

/// `bytes + bytes` (concat) and `bytes * int` (repetition). Mirrors
/// the list/str shape — defer the actual work to `apply_binop_builtin`
/// which routes through `add_values` / `mult_values`.
fn bytes_arith(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    _decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    let Value::Bytes(_) = lhs else { return None };
    match op {
        BinOp::Add if matches!(rhs, Value::Bytes(_)) => {
            Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs))
        }
        BinOp::Mul if matches!(rhs, Value::Int(_) | Value::Bool(_)) => {
            Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs))
        }
        _ => None,
    }
}

/// `list + list` (concat) and `list * int` (repetition).
fn list_arith(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    _decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    let Value::List(_) = lhs else { return None };
    match op {
        BinOp::Add if matches!(rhs, Value::List(_)) => {
            Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs))
        }
        BinOp::Mul if matches!(rhs, Value::Int(_) | Value::Bool(_)) => {
            Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs))
        }
        _ => None,
    }
}

/// `tuple + tuple` (concat) and `tuple * int` (repetition).
fn tuple_arith(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    _decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    let Value::Tuple(_) = lhs else { return None };
    match op {
        BinOp::Add if matches!(rhs, Value::Tuple(_)) => {
            Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs))
        }
        BinOp::Mul if matches!(rhs, Value::Int(_) | Value::Bool(_)) => {
            Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs))
        }
        _ => None,
    }
}

/// `set - set` (difference). Other set operators (`|`, `&`, `^`) are
/// bitwise ops on the legacy `apply_binop` path until A3's follow-up.
fn set_arith(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    _decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    let Value::Set(_) = lhs else { return None };
    match op {
        BinOp::Sub if matches!(rhs, Value::Set(_)) => {
            Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs))
        }
        _ => None,
    }
}

const fn is_numeric(v: &Value) -> bool {
    matches!(v, Value::Int(_) | Value::BigInt(_) | Value::Float(_) | Value::Bool(_))
}

// ---------------------------------------------------------------------------
// Iteration slot implementations
// ---------------------------------------------------------------------------

/// `iter(list)`/`iter(tuple)`/`iter(set)` — materialize the underlying Vec.
/// All three variants share the same payload shape, so one slot covers
/// them. The clone is the load-bearing cost; lazy iteration through a
/// `Value::Iterator` variant is the follow-up that lands `iter()`/`next()`
/// builtins.
#[expect(
    clippy::unnecessary_wraps,
    reason = "IterSlot protocol fixes the Result<Vec<Value>, EvalError> signature; same-type iter slots always succeed but keep the protocol so call sites stay homogeneous"
)]
fn sequence_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    // List is shared via Arc<Mutex<Vec>>; clone the inner Vec contents
    // under the lock so the iteration sees a snapshot. Tuple/Set still
    // wrap a plain Vec and clone directly.
    if let Value::List(items) = value {
        return Ok(items.lock().clone());
    }
    let (Value::Tuple(items) | Value::Set(items)) = value else {
        unreachable!("sequence_iter only attached to list/tuple/set TypeObjects")
    };
    Ok(items.clone())
}

/// `iter(str)` — yield single-char strings, matching CPython's str iteration.
#[expect(
    clippy::unnecessary_wraps,
    reason = "IterSlot protocol; str iteration cannot fail at the materialization step"
)]
fn str_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    let Value::String(s) = value else { unreachable!("str_iter only on STR_TYPE") };
    Ok(s.chars().map(|c| Value::String(c.to_string().into())).collect())
}

/// `iter(bytes)` — yield the integer byte values, matching CPython's bytes
/// iteration (`for b in b"abc"` gives ints `[97, 98, 99]`).
#[expect(clippy::unnecessary_wraps, reason = "IterSlot protocol; bytes iteration cannot fail")]
fn bytes_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    let Value::Bytes(b) = value else { unreachable!("bytes_iter only on BYTES_TYPE") };
    Ok(b.iter().map(|&byte| Value::Int(i64::from(byte))).collect())
}

/// `iter(dict)` — yield the keys, matching CPython's dict iteration.
#[expect(clippy::unnecessary_wraps, reason = "IterSlot protocol; dict iteration cannot fail")]
fn dict_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    let Value::Dict(map) = value else { unreachable!("dict_iter only on DICT_TYPE") };
    Ok(map.keys().map(crate::value::ValueKey::to_value).collect())
}

/// `iter(range)` — materialize the arithmetic progression as a `Vec<Int>`.
/// Step is validated at range-construction time (step != 0); the loop here
/// just walks until the stop bound respecting the sign.
#[expect(
    clippy::unnecessary_wraps,
    reason = "IterSlot protocol; range walk cannot fail (step != 0 is enforced at construction)"
)]
fn range_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    let Value::Range { start, stop, step } = value else {
        unreachable!("range_iter only on RANGE_TYPE")
    };
    let mut items = Vec::new();
    let mut i = *start;
    match (*step).cmp(&0) {
        std::cmp::Ordering::Greater => {
            while i < *stop {
                items.push(Value::Int(i));
                i += step;
            }
        }
        std::cmp::Ordering::Less => {
            while i > *stop {
                items.push(Value::Int(i));
                i += step;
            }
        }
        std::cmp::Ordering::Equal => {}
    }
    Ok(items)
}

/// `x in range(start, stop, step)` — step-aware modular membership check,
/// O(1) rather than walking the materialized range. CPython treats
/// integer-valued floats and bools as equivalent to their int form
/// (`1.0 in range(5)` is True), so coerce before the modular check.
/// Non-numeric / non-integer-valued items short-circuit to False.
#[expect(
    clippy::unnecessary_wraps,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    reason = "ContainsSlot protocol; the round-trip-guarded float→int fold matches CPython's bool/float/int numeric equivalence"
)]
fn range_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let Value::Range { start, stop, step } = container else {
        unreachable!("range_contains only on RANGE_TYPE")
    };
    let val: i64 = match item {
        Value::Int(n) => *n,
        Value::Bool(b) => i64::from(*b),
        Value::Float(f) => {
            if !f.is_finite() || f.fract() != 0.0 {
                return Ok(false);
            }
            let as_int = *f as i64;
            if as_int as f64 != *f {
                return Ok(false);
            }
            as_int
        }
        _ => return Ok(false),
    };
    if *step == 0 {
        return Ok(false);
    }
    let in_bounds =
        if *step > 0 { val >= *start && val < *stop } else { val <= *start && val > *stop };
    Ok(in_bounds && (val - *start) % *step == 0)
}

// ---------------------------------------------------------------------------
// Hash slot implementations — port of CPython's per-type hashers
// ---------------------------------------------------------------------------

/// CPython hash-output width: `Py_hash_t` is signed 64-bit; modular reduction
/// uses the Mersenne prime `2^61 - 1`. Source:
/// `Include/internal/pycore_pyhash.h`.
const HASH_BITS: u32 = 61;
const HASH_MODULUS: u64 = (1u64 << HASH_BITS) - 1;
/// CPython's sentinel hash for positive infinity. Source: `Python/pyhash.c`.
const HASH_INF: i64 = 314_159;

/// CPython substitutes `-2` for `-1` so `-1` can stay reserved as the
/// "uncomputed" sentinel inside the runtime. Source: `Python/pyhash.c`.
const fn finalize_hash(h: i64) -> i64 {
    if h == -1 { -2 } else { h }
}

const fn none_hash(_value: &Value) -> i64 {
    // CPython 3.12 returns a deterministic constant for `hash(None)` (the
    // address of `Py_None`'s singleton, stable across runs). 0 is the
    // platform-independent choice that preserves `hash(None) == hash(None)`
    // and never collides with `hash(0)` after `finalize_hash` (0 stays 0).
    0
}

fn bool_hash(value: &Value) -> i64 {
    let Value::Bool(b) = value else { unreachable!("bool_hash sees only Value::Bool") };
    // `hash(True) == hash(1)` and `hash(False) == hash(0)` per CPython —
    // bool is a subclass of int, so its hash IS the int hash.
    finalize_hash(int_hash_impl(i64::from(*b)))
}

fn int_hash_slot(value: &Value) -> i64 {
    match value {
        Value::Int(n) => finalize_hash(int_hash_impl(*n)),
        Value::BigInt(n) => {
            // Reduce modulo HASH_MODULUS like CPython's long_hash.
            use num_traits::{Signed, ToPrimitive as _};
            let modulus = num_bigint::BigInt::from(HASH_MODULUS);
            let mut rem = n.abs() % &modulus;
            if n.sign() == num_bigint::Sign::Minus {
                rem = -rem;
            }
            finalize_hash(rem.to_i64().unwrap_or(0))
        }
        _ => unreachable!("int_hash_slot sees only int variants"),
    }
}

#[expect(
    clippy::cast_possible_wrap,
    reason = "abs is bounded by HASH_MODULUS (~2^61), well within i64::MAX; the cast is sign-preserving"
)]
const fn int_hash_impl(n: i64) -> i64 {
    let abs = n.unsigned_abs() % HASH_MODULUS;
    if n < 0 { -(abs as i64) } else { abs as i64 }
}

fn float_hash_slot(value: &Value) -> i64 {
    let Value::Float(f) = value else { unreachable!("float_hash_slot sees only Value::Float") };
    finalize_hash(float_hash_impl(*f))
}

/// `_Py_HashDouble` for `f64`. Operates on the magnitude via `frexp`,
/// processes 28 mantissa bits at a time through a rotating accumulator modulo
/// `HASH_MODULUS`, then realigns by the exponent. Sign is re-applied at the
/// end. Source: `Python/pyhash.c::_Py_HashDouble`.
#[expect(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "translation of CPython's _Py_HashDouble — every cast mirrors the C version's semantics and operates on bounded values"
)]
#[expect(
    clippy::many_single_char_names,
    reason = "matches CPython's _Py_HashDouble variable names verbatim (m mantissa, e exponent, x accumulator, y integer-part-of-shifted-mantissa, v input) for line-by-line traceability against Python/pyhash.c"
)]
#[expect(
    clippy::while_float,
    reason = "termination follows CPython's invariant that the 28-bit-per-iteration shift drains the mantissa to exact 0.0 within ceil(53/28) iterations on a finite f64"
)]
fn float_hash_impl(v: f64) -> i64 {
    if !v.is_finite() {
        if v.is_infinite() {
            return if v > 0.0 { HASH_INF } else { -HASH_INF };
        }
        // NaN: CPython returns an id-based hash; we have no object identity
        // here, so return 0. `hash(float('nan'))` is the only surface that
        // observes this and the value itself is implementation-defined per
        // CPython's own docs.
        return 0;
    }

    let sign: i64 = if v < 0.0 { -1 } else { 1 };
    let (mut m, mut e) = frexp(v.abs());
    let mut x: u64 = 0;
    while m != 0.0 {
        x = ((x << 28) & HASH_MODULUS) | (x >> (HASH_BITS - 28));
        m *= 268_435_456.0; // 2^28
        e -= 28;
        let y = m as u64;
        m -= y as f64;
        x = x.wrapping_add(y);
        if x >= HASH_MODULUS {
            x -= HASH_MODULUS;
        }
    }

    let e_adj: u32 = if e >= 0 {
        (e as u32) % HASH_BITS
    } else {
        HASH_BITS - 1 - (((-1 - e) as u32) % HASH_BITS)
    };
    x = ((x << e_adj) & HASH_MODULUS) | (x >> (HASH_BITS - e_adj));

    (x as i64).wrapping_mul(sign)
}

/// Decompose `v` into `(m, e)` such that `v = m * 2^e` with `0.5 <= |m| < 1`.
/// Implemented via direct bit manipulation of the IEEE 754 representation so
/// the decomposition is exact and matches CPython's libc `frexp` output.
fn frexp(v: f64) -> (f64, i32) {
    if v == 0.0 || !v.is_finite() {
        return (v, 0);
    }
    let bits = v.to_bits();
    let biased_exp = ((bits >> 52) & 0x7FF) as i32;
    if biased_exp == 0 {
        // Subnormal: scale into the normal range, then offset the exponent.
        let scaled = v * f64::from_bits((1023u64 + 54) << 52); // 2^54
        let (m, e) = frexp(scaled);
        return (m, e - 54);
    }
    let new_bits = (bits & !(0x7FFu64 << 52)) | (1022u64 << 52);
    let m = f64::from_bits(new_bits);
    let e = biased_exp - 1022;
    (m, e)
}

/// Fallback hash slot for types we haven't ported to CPython's exact
/// algorithm (str, bytes, tuple, plus the OBJECT_TYPE catch-all). Uses
/// the existing `value_to_key` + `DefaultHasher` route. The hash
/// values diverge from CPython's reference implementation (SipHash
/// with seed 0); they're stable across runs within this interpreter
/// only.
#[expect(
    clippy::cast_possible_wrap,
    reason = "Python's hash() returns a signed integer; reinterpreting u64 bits as i64 via wrapping matches CPython's Py_hash_t on 64-bit platforms"
)]
fn fallback_hash_slot(value: &Value) -> i64 {
    use std::hash::{Hash as _, Hasher as _};
    // `value_to_key` would error on unhashable; we shouldn't reach here
    // unless the value's TypeObject said it's hashable, so the unwrap-or-0
    // fallback is defensive against a misconfigured slot.
    let Ok(key) = crate::eval::literals::value_to_key(value) else {
        return 0;
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    finalize_hash(hasher.finish() as i64)
}

// ---------------------------------------------------------------------------
// Item-access slot implementations
// ---------------------------------------------------------------------------

/// `list[i]` / `tuple[i]` / `set[i]`: positional index access. Set has its
/// own `get_item_slot = None` (CPython sets are not subscriptable); this fn
/// only sees list/tuple via the slot wiring. Bool is treated as int per
/// CPython (`lst[True]` is `lst[1]`).
fn sequence_get_item(container: &Value, index: &Value) -> Result<Value, EvalError> {
    if let Value::List(items) = container {
        let guard = items.lock();
        let raw = int_index(index, "list")?;
        let idx = normalize_seq_index(raw, guard.len(), "list")?;
        return Ok(guard[idx].clone());
    }
    let Value::Tuple(items) = container else {
        unreachable!("sequence_get_item only on list/tuple TypeObjects")
    };
    let raw = int_index(index, "tuple")?;
    let idx = normalize_seq_index(raw, items.len(), "tuple")?;
    Ok(items[idx].clone())
}

/// `str[i]`: index a single character. Operates on the `chars()` iterator
/// (codepoints), matching CPython's str-as-sequence-of-codepoints model.
fn str_get_item(container: &Value, index: &Value) -> Result<Value, EvalError> {
    let Value::String(s) = container else { unreachable!("str_get_item only on STR_TYPE") };
    let raw = int_index(index, "string")?;
    let chars: Vec<char> = s.chars().collect();
    let idx = normalize_seq_index(raw, chars.len(), "string")?;
    Ok(Value::String(chars[idx].to_string().into()))
}

/// `bytes[i]`: index yields the integer byte value, matching CPython
/// (`b"abc"[0]` is `97`).
fn bytes_get_item(container: &Value, index: &Value) -> Result<Value, EvalError> {
    let Value::Bytes(b) = container else { unreachable!("bytes_get_item only on BYTES_TYPE") };
    let raw = int_index(index, "bytes")?;
    let idx = normalize_seq_index(raw, b.len(), "bytes")?;
    Ok(Value::Int(i64::from(b[idx])))
}

/// `dict[key]`: hash-keyed lookup. On miss, consults the type's
/// `missing_slot` (Counter sets it; plain dict leaves it None) before
/// raising `KeyError`.
fn dict_get_item(container: &Value, index: &Value) -> Result<Value, EvalError> {
    let Value::Dict(map) = container else { unreachable!("dict_get_item only on DICT_TYPE") };
    let key = crate::eval::literals::value_to_key(index)?;
    if let Some(value) = map.get(&key) {
        return Ok(value.clone());
    }
    if let Some(missing) = type_of(container).missing_slot {
        return missing(container, index);
    }
    Err(crate::value::ExceptionValue::key_error(key).into())
}

/// `range(start, stop, step)[i]`: arithmetic-progression indexing.
/// Negative indices count from the end; out-of-range raises `IndexError`.
fn range_get_item(container: &Value, index: &Value) -> Result<Value, EvalError> {
    let Value::Range { start, stop, step } = container else {
        unreachable!("range_get_item only on RANGE_TYPE")
    };
    let raw = int_index(index, "range")?;
    let len = range_length(*start, *stop, *step);
    let idx = normalize_seq_index(raw, len, "range object")?;
    let idx_i64 = i64::try_from(idx)
        .map_err(|_| EvalError::from(InterpreterError::Runtime("range index overflow".into())))?;
    Ok(Value::Int(start + idx_i64 * step))
}

/// `list[i] = value`: replace the element at index `i`. Returns the
/// signed byte delta on the container.
fn list_set_item(container: &mut Value, index: &Value, value: Value) -> Result<isize, EvalError> {
    let Value::List(items) = container else { unreachable!("list_set_item only on LIST_TYPE") };
    let raw = int_index(index, "list")?;
    let mut guard = items.lock();
    let idx = normalize_seq_index(raw, guard.len(), "list")?;
    let delta = size_delta(
        crate::state::estimate_value_size(&guard[idx]),
        crate::state::estimate_value_size(&value),
    );
    guard[idx] = value;
    drop(guard);
    Ok(delta)
}

/// `dict[key] = value`: insert or overwrite. Returns the signed byte
/// delta (overwrite = value-size delta; insert = key + value).
fn dict_set_item(container: &mut Value, index: &Value, value: Value) -> Result<isize, EvalError> {
    let Value::Dict(map) = container else { unreachable!("dict_set_item only on DICT_TYPE") };
    let key = crate::eval::literals::value_to_key(index)?;
    let new_size = crate::state::estimate_value_size(&value);
    let delta = map.insert(key.clone(), value).map_or_else(
        || to_isize_sat(crate::state::estimate_key_size(&key) + new_size),
        |old| size_delta(crate::state::estimate_value_size(&old), new_size),
    );
    Ok(delta)
}

/// `del list[i]`: remove the element at index `i`, shifting tail down.
/// Returns the (negative) byte delta.
fn list_del_item(container: &mut Value, index: &Value) -> Result<isize, EvalError> {
    let Value::List(items) = container else { unreachable!("list_del_item only on LIST_TYPE") };
    let raw = int_index(index, "list")?;
    let mut guard = items.lock();
    let idx = normalize_seq_index(raw, guard.len(), "list")?;
    let removed = guard.remove(idx);
    drop(guard);
    Ok(-to_isize_sat(crate::state::estimate_value_size(&removed)))
}

/// `del dict[key]`: hash-keyed remove. Raises `KeyError` on miss.
fn dict_del_item(container: &mut Value, index: &Value) -> Result<isize, EvalError> {
    let Value::Dict(map) = container else { unreachable!("dict_del_item only on DICT_TYPE") };
    let key = crate::eval::literals::value_to_key(index)?;
    let Some(val) = map.swap_remove(&key) else {
        return Err(crate::value::ExceptionValue::key_error(key).into());
    };
    let freed = crate::state::estimate_key_size(&key) + crate::state::estimate_value_size(&val);
    Ok(-to_isize_sat(freed))
}

/// Coerce a subscript index to `i64`, accepting int and bool (CPython
/// treats bool as int subclass). All other types raise `TypeError`.
fn int_index(index: &Value, container_name: &str) -> Result<i64, EvalError> {
    match index {
        Value::Int(i) => Ok(*i),
        Value::Bool(b) => Ok(i64::from(*b)),
        other => Err(InterpreterError::TypeError(format!(
            "{container_name} indices must be integers, not '{}'",
            other.type_name()
        ))
        .into()),
    }
}

/// Normalize a Python sequence index (negative = from the end) into a
/// `usize`. Raises CPython's `IndexError` shape on out-of-range with a
/// type-specific body (`list index out of range`, `tuple index out of
/// range`, `string index out of range`, ...) — CPython's wording
/// varies by container, and a planner LLM matches the exact phrase
/// when picking a repair.
fn normalize_seq_index(raw: i64, len: usize, kind: &str) -> Result<usize, EvalError> {
    let len_i = i64::try_from(len).map_err(|_| {
        EvalError::from(InterpreterError::Runtime(
            "sequence length overflows i64 for indexing".into(),
        ))
    })?;
    let adjusted = if raw < 0 { len_i + raw } else { raw };
    if adjusted < 0 || adjusted >= len_i {
        return Err(crate::value::ExceptionValue::index_error(kind).into());
    }
    usize::try_from(adjusted).map_err(|_| {
        EvalError::from(InterpreterError::Runtime("index overflow (internal invariant)".into()))
    })
}

/// Signed `new - old` byte delta, saturating rather than wrapping.
const fn size_delta(old: usize, new: usize) -> isize {
    to_isize_sat(new).saturating_sub(to_isize_sat(old))
}

/// Convert a byte count into `isize`, saturating at `isize::MAX`. Sizes
/// are bounded by the memory limit, so this never clamps in practice — it
/// is the lint-clean conversion at the boundary.
#[expect(
    clippy::cast_possible_wrap,
    reason = "guarded by the if-check above: n <= isize::MAX before the cast, so the resulting i64 sign bit is always 0"
)]
const fn to_isize_sat(n: usize) -> isize {
    if n > isize::MAX as usize { isize::MAX } else { n as isize }
}

// ---------------------------------------------------------------------------
// Length slot implementations
// ---------------------------------------------------------------------------

/// `len(list)` / `len(tuple)` / `len(set)`: items count.
#[expect(clippy::unnecessary_wraps, reason = "LenSlot protocol fixes the Result signature")]
fn sequence_len(value: &Value) -> Result<usize, EvalError> {
    if let Value::List(items) = value {
        return Ok(items.lock().len());
    }
    let (Value::Tuple(items) | Value::Set(items)) = value else {
        unreachable!("sequence_len only on list/tuple/set TypeObjects")
    };
    Ok(items.len())
}

/// `len(str)`: codepoint count, matching CPython (a Python str is a
/// sequence of Unicode codepoints, not bytes).
#[expect(clippy::unnecessary_wraps, reason = "LenSlot protocol")]
fn str_len(value: &Value) -> Result<usize, EvalError> {
    let Value::String(s) = value else { unreachable!("str_len only on STR_TYPE") };
    Ok(s.chars().count())
}

#[expect(clippy::unnecessary_wraps, reason = "LenSlot protocol")]
fn bytes_len(value: &Value) -> Result<usize, EvalError> {
    let Value::Bytes(b) = value else { unreachable!("bytes_len only on BYTES_TYPE") };
    Ok(b.len())
}

#[expect(clippy::unnecessary_wraps, reason = "LenSlot protocol")]
fn dict_len(value: &Value) -> Result<usize, EvalError> {
    let Value::Dict(map) = value else { unreachable!("dict_len only on DICT_TYPE") };
    Ok(map.len())
}

/// `len(range)`: closed-form arithmetic.
#[expect(clippy::unnecessary_wraps, reason = "LenSlot protocol; range length cannot fail")]
fn range_len(value: &Value) -> Result<usize, EvalError> {
    let Value::Range { start, stop, step } = value else {
        unreachable!("range_len only on RANGE_TYPE")
    };
    Ok(range_length(*start, *stop, *step))
}

/// Closed-form `len(range(start, stop, step))` — ceil((stop - start) /
/// step) clamped to zero. Step of 0 returns 0 (defensive; range
/// construction already rejects step=0 with `ValueError`).
fn range_length(start: i64, stop: i64, step: i64) -> usize {
    let raw = match step.cmp(&0) {
        std::cmp::Ordering::Greater => ((stop - start + step - 1) / step).max(0),
        std::cmp::Ordering::Less => ((start - stop - step - 1) / (-step)).max(0),
        std::cmp::Ordering::Equal => 0,
    };
    usize::try_from(raw).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Attribute-access slot implementations
// ---------------------------------------------------------------------------

/// Built-in instance method names for each sequence-like type. Method
/// dispatch happens in `eval/functions.rs` — these tables exist so an
/// attribute lookup returns a method-marker sentinel rather than an
/// `AttributeError` for valid method names.
const DICT_METHODS: &[&str] =
    &["keys", "values", "items", "get", "pop", "update", "setdefault", "copy", "clear"];

const STR_METHODS: &[&str] = &[
    "upper",
    "lower",
    "strip",
    "lstrip",
    "rstrip",
    "split",
    "rsplit",
    "join",
    "replace",
    "startswith",
    "endswith",
    "removeprefix",
    "removesuffix",
    "casefold",
    "encode",
    "expandtabs",
    "partition",
    "rpartition",
    "find",
    "rfind",
    "index",
    "count",
    "format",
    "isdigit",
    "isalpha",
    "isalnum",
    "isspace",
    "isupper",
    "islower",
    "title",
    "capitalize",
    "swapcase",
    "center",
    "ljust",
    "rjust",
    "zfill",
    "encode",
];

const LIST_METHODS: &[&str] = &[
    "append", "extend", "insert", "pop", "remove", "sort", "reverse", "index", "count", "copy",
    "clear",
];

const TUPLE_METHODS: &[&str] = &["count", "index"];

const SET_METHODS: &[&str] = &[
    "add",
    "remove",
    "discard",
    "pop",
    "clear",
    "copy",
    "union",
    "intersection",
    "difference",
    "symmetric_difference",
    "issubset",
    "issuperset",
    "isdisjoint",
    "update",
    "intersection_update",
    "difference_update",
    "symmetric_difference_update",
];

/// Build a bound-method value with a snapshot receiver: a builtin
/// method captured together with a clone of its receiver. Returned by
/// `_get_attr` slots when the user reads `obj.method` as a value (e.g.
/// `key=d.get`) rather than invoking it inline.
///
/// The type-slot dispatch path that calls this helper does not know
/// whether the receiver came from a place expression — by the time
/// `_get_attr` runs, the receiver has already been evaluated to a
/// Value. `eval_attribute` upgrades Snapshot→Place when the original
/// receiver expression was a navigable place, so this helper's
/// snapshot semantics is the correct default for non-place receivers
/// (literals, function results).
fn bound_method(value: &Value, attr_name: &str) -> Value {
    Value::BoundMethod {
        receiver: crate::value::BoundMethodReceiver::Snapshot(Box::new(value.clone())),
        method: attr_name.to_string(),
    }
}

/// Build an `AttributeError` for `'<type>' object has no attribute '<attr>'`.
fn attribute_error(type_name: &str, attr_name: &str) -> EvalError {
    InterpreterError::AttributeError(format!("'{type_name}' object has no attribute '{attr_name}'"))
        .into()
}

/// Catch-all `get_attr_slot` for types with no attributes (None, bool,
/// int, float, bytes, range). Always raises `AttributeError`.
fn noattr_get_attr(value: &Value, name: &str) -> EvalResult {
    Err(attribute_error(value.type_name(), name))
}

/// `dict.attr`: method dispatch only. CPython does not expose dict
/// keys as attributes — `d.foo` raises `AttributeError` regardless of
/// whether `"foo"` is a key (use `d["foo"]` instead). The pre-A6
/// implementation in `eval/names.rs` did key-lookup first, which was a
/// hidden footgun where a tool-returned dict whose key happened to be
/// `"keys"` would silently mask the method. A6 closes this divergence.
fn dict_get_attr(value: &Value, name: &str) -> EvalResult {
    if DICT_METHODS.contains(&name) {
        return Ok(bound_method(value, name));
    }
    Err(attribute_error("dict", name))
}

/// `str.attr`: method dispatch only (str has no instance attributes
/// beyond methods). Returns a bound method for valid method names so
/// `s.upper`, `map(str.upper, items)` etc. work as first-class callables.
fn str_get_attr(value: &Value, name: &str) -> EvalResult {
    if STR_METHODS.contains(&name) {
        return Ok(bound_method(value, name));
    }
    Err(attribute_error("str", name))
}

fn list_get_attr(value: &Value, name: &str) -> EvalResult {
    if LIST_METHODS.contains(&name) {
        return Ok(bound_method(value, name));
    }
    Err(attribute_error("list", name))
}

fn tuple_get_attr(value: &Value, name: &str) -> EvalResult {
    if TUPLE_METHODS.contains(&name) {
        return Ok(bound_method(value, name));
    }
    Err(attribute_error("tuple", name))
}

fn set_get_attr(value: &Value, name: &str) -> EvalResult {
    if SET_METHODS.contains(&name) {
        return Ok(bound_method(value, name));
    }
    Err(attribute_error("set", name))
}

// ---------------------------------------------------------------------------
// Error-builder helper kept here so it co-locates with the dispatch caller.
// ---------------------------------------------------------------------------
// Counter slot implementations//
// ---------------------------------------------------------------------------

/// `Counter == Counter` / `Counter == dict`: counter equality matches
/// CPython — equal iff same set of (key, count) entries. Comparison
/// against a plain dict succeeds when the maps' contents match (Counter
/// is a dict subclass).
fn counter_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::Counter(a) = lhs else { return None };
    match rhs {
        Value::Counter(b) | Value::Dict(b) => {
            if a.len() != b.len() {
                return Some(false);
            }
            Some(a.iter().all(|(k, v)| b.get(k).is_some_and(|bv| recurse_eq(v, bv))))
        }
        _ => None,
    }
}

/// `key in counter`: same hash-keyed lookup as dict. A Counter
/// reports membership based on stored entries, not non-zero values
/// — `c["missing"] == 0` but `"missing" in c` is False (matching
/// CPython).
#[expect(clippy::unnecessary_wraps, reason = "ContainsSlot protocol")]
fn counter_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let Value::Counter(map) = container else {
        unreachable!("counter_contains only on COUNTER_TYPE")
    };
    let Ok(key) = crate::eval::literals::value_to_key(item) else {
        return Ok(false);
    };
    Ok(map.contains_key(&key))
}

/// `iter(counter)` yields keys — same as dict.
#[expect(clippy::unnecessary_wraps, reason = "IterSlot protocol")]
fn counter_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    let Value::Counter(map) = value else { unreachable!("counter_iter only on COUNTER_TYPE") };
    Ok(map.keys().map(crate::value::ValueKey::to_value).collect())
}

/// `counter[key]`: hash lookup. On miss, returns the value from the
/// `missing_slot` (Int(0)) without inserting — that's the load-bearing
/// distinction from plain dict.
fn counter_get_item(container: &Value, index: &Value) -> Result<Value, EvalError> {
    let Value::Counter(map) = container else {
        unreachable!("counter_get_item only on COUNTER_TYPE")
    };
    let key = crate::eval::literals::value_to_key(index)?;
    if let Some(value) = map.get(&key) {
        return Ok(value.clone());
    }
    if let Some(missing) = type_of(container).missing_slot {
        return missing(container, index);
    }
    Err(crate::value::ExceptionValue::key_error(key).into())
}

/// `counter[key] = value`: insert or overwrite. Same memory accounting
/// as `dict_set_item`.
fn counter_set_item(
    container: &mut Value,
    index: &Value,
    value: Value,
) -> Result<isize, EvalError> {
    let Value::Counter(map) = container else {
        unreachable!("counter_set_item only on COUNTER_TYPE")
    };
    let key = crate::eval::literals::value_to_key(index)?;
    let new_size = crate::state::estimate_value_size(&value);
    let delta = map.insert(key.clone(), value).map_or_else(
        || to_isize_sat(crate::state::estimate_key_size(&key) + new_size),
        |old| size_delta(crate::state::estimate_value_size(&old), new_size),
    );
    Ok(delta)
}

/// `del counter[key]`: hash-keyed remove. Raises KeyError on miss
/// (the `__missing__` hook is for read-on-miss only, not delete).
fn counter_del_item(container: &mut Value, index: &Value) -> Result<isize, EvalError> {
    let Value::Counter(map) = container else {
        unreachable!("counter_del_item only on COUNTER_TYPE")
    };
    let key = crate::eval::literals::value_to_key(index)?;
    let Some(val) = map.swap_remove(&key) else {
        return Err(crate::value::ExceptionValue::key_error(key).into());
    };
    let freed = crate::state::estimate_key_size(&key) + crate::state::estimate_value_size(&val);
    Ok(-to_isize_sat(freed))
}

/// Counter's `__missing__`: returns Int(0) WITHOUT inserting. CPython's
/// `Counter.__missing__` does exactly this; calling code that does
/// `c[key]` reads 0 but doesn't materialise an entry.
#[expect(clippy::unnecessary_wraps, reason = "MissingSlot protocol")]
const fn counter_missing(_container: &Value, _key: &Value) -> Result<Value, EvalError> {
    Ok(Value::Int(0))
}

/// `len(counter)`: entry count, same as dict.
#[expect(clippy::unnecessary_wraps, reason = "LenSlot protocol")]
fn counter_len(value: &Value) -> Result<usize, EvalError> {
    let Value::Counter(map) = value else { unreachable!("counter_len only on COUNTER_TYPE") };
    Ok(map.len())
}

/// `counter.attr`: method dispatch. Counter inherits dict's method
/// surface (keys/values/items/get/pop/copy/clear) plus its own
/// most_common, elements, subtract, update. We expose them via
/// method-marker sentinels that the call evaluator's dispatch_method
/// recognises.
fn counter_get_attr(value: &Value, name: &str) -> EvalResult {
    const COUNTER_METHODS: &[&str] = &[
        // Inherited from dict
        "keys",
        "values",
        "items",
        "get",
        "pop",
        "copy",
        "clear",
        "setdefault",
        // Counter's own
        "most_common",
        "elements",
        "subtract",
        "update",
        "total",
    ];
    if COUNTER_METHODS.contains(&name) {
        return Ok(bound_method(value, name));
    }
    Err(attribute_error("Counter", name))
}

/// Multiset arithmetic: + - & |. CPython's Counter inherits +/- from
/// dict (which raises TypeError) but overrides them to mean multiset
/// add / subtract. & is intersection (min of counts), | is union
/// (max). All four KEEP ONLY positive results.
fn counter_arith(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    _decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    let Value::Counter(a) = lhs else { return None };
    let Value::Counter(b) = rhs else { return None };
    match op {
        BinOp::Add => Some(Ok(Value::Counter(counter_combine_op(a, b, |x, y| x + y)))),
        BinOp::Sub => Some(Ok(Value::Counter(counter_combine_op(a, b, |x, y| x - y)))),
        _ => None,
    }
}

/// Combine two counter maps by applying `op` to overlapping entries
/// and keeping `a`'s entries (with `op(value, 0)`) where `b` is
/// absent, then `b`'s entries similarly. Keeps only strictly positive
/// results — matches CPython's `_keep_positive` filter for +/- /& /|.
pub(crate) fn counter_combine_op(
    a: &indexmap::IndexMap<crate::value::ValueKey, Value>,
    b: &indexmap::IndexMap<crate::value::ValueKey, Value>,
    op: fn(i64, i64) -> i64,
) -> indexmap::IndexMap<crate::value::ValueKey, Value> {
    let mut result = indexmap::IndexMap::new();
    for (key, av) in a {
        let ax = counter_int(av);
        let bx = b.get(key).map_or(0, counter_int);
        let r = op(ax, bx);
        if r > 0 {
            result.insert(key.clone(), Value::Int(r));
        }
    }
    for (key, bv) in b {
        if a.contains_key(key) {
            continue;
        }
        let r = op(0, counter_int(bv));
        if r > 0 {
            result.insert(key.clone(), Value::Int(r));
        }
    }
    result
}

/// Coerce a counter value to i64. Counters store Int values but
/// CPython tolerates float counts; non-numeric falls through to 0.
fn counter_int(value: &Value) -> i64 {
    match value {
        Value::Int(n) => *n,
        Value::Bool(b) => i64::from(*b),
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// deque + defaultdict slot impls.
// ---------------------------------------------------------------------------

/// Equality slot that returns `NotImplemented` for every input — the
/// caller raises TypeError if both sides return None. Used by Deque
/// and DefaultDict whose CPython equality is dict-like but is more
/// involved than needed in our eager-extract workload (CPython's deque
/// == deque compares element-wise; we keep that as a follow-up).
const fn noimpl_eq(_lhs: &Value, _rhs: &Value) -> Option<bool> {
    None
}

/// `x in deque` — linear scan with eq_dispatch.
#[expect(clippy::unnecessary_wraps, reason = "ContainsSlot protocol")]
fn deque_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let Value::Deque { items, .. } = container else {
        unreachable!("deque_contains only on DEQUE_TYPE")
    };
    Ok(items.iter().any(|entry| recurse_eq(item, entry)))
}

/// `iter(deque)` — materialise to Vec.
#[expect(clippy::unnecessary_wraps, reason = "IterSlot protocol")]
fn deque_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    let Value::Deque { items, .. } = value else { unreachable!("deque_iter only on DEQUE_TYPE") };
    Ok(items.iter().cloned().collect())
}

/// `deque[i]` — positional index; bool / negative indices supported.
fn deque_get_item(container: &Value, index: &Value) -> Result<Value, EvalError> {
    let Value::Deque { items, .. } = container else {
        unreachable!("deque_get_item only on DEQUE_TYPE")
    };
    let raw = int_index(index, "deque")?;
    let idx = normalize_seq_index(raw, items.len(), "deque")?;
    Ok(items[idx].clone())
}

#[expect(clippy::unnecessary_wraps, reason = "LenSlot protocol")]
fn deque_len(value: &Value) -> Result<usize, EvalError> {
    let Value::Deque { items, .. } = value else { unreachable!("deque_len only on DEQUE_TYPE") };
    Ok(items.len())
}

/// `deque.attr` — method dispatch table (no instance attributes).
fn deque_get_attr(value: &Value, name: &str) -> EvalResult {
    const DEQUE_METHODS: &[&str] = &[
        "append",
        "appendleft",
        "pop",
        "popleft",
        "extend",
        "extendleft",
        "rotate",
        "clear",
        "copy",
    ];
    if DEQUE_METHODS.contains(&name) {
        return Ok(bound_method(value, name));
    }
    Err(attribute_error("deque", name))
}

/// `key in defaultdict` — same as dict.
#[expect(clippy::unnecessary_wraps, reason = "ContainsSlot protocol")]
fn defaultdict_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let Value::DefaultDict(data) = container else {
        unreachable!("defaultdict_contains only on DEFAULTDICT_TYPE")
    };
    let Ok(key) = crate::eval::literals::value_to_key(item) else {
        return Ok(false);
    };
    Ok(data.items.contains_key(&key))
}

#[expect(clippy::unnecessary_wraps, reason = "IterSlot protocol")]
fn defaultdict_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    let Value::DefaultDict(data) = value else {
        unreachable!("defaultdict_iter only on DEFAULTDICT_TYPE")
    };
    Ok(data.items.keys().map(crate::value::ValueKey::to_value).collect())
}

fn defaultdict_set_item(
    container: &mut Value,
    index: &Value,
    value: Value,
) -> Result<isize, EvalError> {
    let Value::DefaultDict(data) = container else {
        unreachable!("defaultdict_set_item only on DEFAULTDICT_TYPE")
    };
    let key = crate::eval::literals::value_to_key(index)?;
    let new_size = crate::state::estimate_value_size(&value);
    let delta = data.items.insert(key.clone(), value).map_or_else(
        || to_isize_sat(crate::state::estimate_key_size(&key) + new_size),
        |old| size_delta(crate::state::estimate_value_size(&old), new_size),
    );
    Ok(delta)
}

fn defaultdict_del_item(container: &mut Value, index: &Value) -> Result<isize, EvalError> {
    let Value::DefaultDict(data) = container else {
        unreachable!("defaultdict_del_item only on DEFAULTDICT_TYPE")
    };
    let key = crate::eval::literals::value_to_key(index)?;
    let Some(val) = data.items.swap_remove(&key) else {
        return Err(crate::value::ExceptionValue::key_error(key).into());
    };
    let freed = crate::state::estimate_key_size(&key) + crate::state::estimate_value_size(&val);
    Ok(-to_isize_sat(freed))
}

#[expect(clippy::unnecessary_wraps, reason = "LenSlot protocol")]
fn defaultdict_len(value: &Value) -> Result<usize, EvalError> {
    let Value::DefaultDict(data) = value else {
        unreachable!("defaultdict_len only on DEFAULTDICT_TYPE")
    };
    Ok(data.items.len())
}

// ---------------------------------------------------------------------------
// Decimal slot implementations
// ---------------------------------------------------------------------------
//
// `BigDecimal` does the exact arithmetic; the slot fns lift `int` /
// `bool` operands into `BigDecimal` so cross-type ops stay exact.
// `Decimal + float` raises `TypeError` per CPython.

fn decimal_to_bigdecimal(value: &Value) -> Option<bigdecimal::BigDecimal> {
    match value {
        Value::Decimal(d) => Some((**d).clone()),
        Value::Int(i) => Some(bigdecimal::BigDecimal::from(*i)),
        Value::BigInt(i) => Some(bigdecimal::BigDecimal::from(i.as_ref().clone())),
        Value::Bool(b) => Some(bigdecimal::BigDecimal::from(i64::from(*b))),
        _ => None,
    }
}

fn decimal_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    Some(decimal_to_bigdecimal(lhs)? == decimal_to_bigdecimal(rhs)?)
}

fn decimal_lt(lhs: &Value, rhs: &Value) -> Option<bool> {
    Some(decimal_to_bigdecimal(lhs)? < decimal_to_bigdecimal(rhs)?)
}

fn decimal_arith(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    use num_traits::Zero as _;
    if matches!(lhs, Value::Float(_)) || matches!(rhs, Value::Float(_)) {
        return Some(Err(InterpreterError::TypeError(
            "unsupported operand type(s) for arithmetic: 'Decimal' and 'float'".into(),
        )
        .into()));
    }
    let (a, b) = (decimal_to_bigdecimal(lhs)?, decimal_to_bigdecimal(rhs)?);
    let result: bigdecimal::BigDecimal = match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => {
            if b.is_zero() {
                return Some(Err(
                    InterpreterError::Runtime("Decimal division by zero".into()).into()
                ));
            }
            let prec = decimal_prec;
            let digits = u64::try_from(prec).unwrap_or(28);
            // Cap significant digits at context prec without padding exact results.
            let q = a / b;
            if q.digits() > digits { q.with_prec(digits) } else { q }
        }
        BinOp::FloorDiv => {
            if b.is_zero() {
                return Some(Err(
                    InterpreterError::Runtime("Decimal division by zero".into()).into()
                ));
            }
            // BigDecimal lacks a direct floor-div; round toward negative
            // infinity by computing the quotient and truncating its
            // fractional part.
            (a / b).with_scale(0)
        }
        // Mod / Pow / others: not yet wired (rare in pipeline workloads).
        _ => return None,
    };
    Some(Ok(Value::Decimal(Box::new(result))))
}

// ---------------------------------------------------------------------------
// Fraction slot implementations
// ---------------------------------------------------------------------------

fn fraction_to_bigrational(value: &Value) -> Option<num_rational::BigRational> {
    use num_bigint::BigInt;
    match value {
        Value::Fraction(f) => Some((**f).clone()),
        Value::Int(i) => Some(num_rational::BigRational::from_integer(BigInt::from(*i))),
        Value::BigInt(i) => Some(num_rational::BigRational::from_integer(i.as_ref().clone())),
        Value::Bool(b) => {
            Some(num_rational::BigRational::from_integer(BigInt::from(i64::from(*b))))
        }
        _ => None,
    }
}

fn fraction_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    Some(fraction_to_bigrational(lhs)? == fraction_to_bigrational(rhs)?)
}

fn fraction_lt(lhs: &Value, rhs: &Value) -> Option<bool> {
    Some(fraction_to_bigrational(lhs)? < fraction_to_bigrational(rhs)?)
}

fn fraction_to_f64(value: &Value) -> Option<f64> {
    use num_traits::ToPrimitive as _;
    match value {
        Value::Float(f) => Some(*f),
        Value::Fraction(f) => f.to_f64(),
        Value::Int(i) => Some(*i as f64),
        Value::BigInt(i) => i.to_f64(),
        Value::Bool(b) => Some(f64::from(*b)),
        _ => None,
    }
}

fn fraction_arith(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    _decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    // CPython: Fraction ±/* float → float.
    if matches!(lhs, Value::Float(_)) || matches!(rhs, Value::Float(_)) {
        let a = fraction_to_f64(lhs)?;
        let b = fraction_to_f64(rhs)?;
        let result = match op {
            BinOp::Add => a + b,
            BinOp::Sub => a - b,
            BinOp::Mul => a * b,
            BinOp::Div => a / b,
            BinOp::FloorDiv => (a / b).floor(),
            BinOp::Mod => a % b,
            BinOp::Pow => a.powf(b),
        };
        return Some(Ok(Value::Float(result)));
    }
    let (a, b) = (fraction_to_bigrational(lhs)?, fraction_to_bigrational(rhs)?);
    let result: num_rational::BigRational = match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => {
            if b.numer().sign() == num_bigint::Sign::NoSign {
                return Some(Err(
                    InterpreterError::Runtime("Fraction division by zero".into()).into()
                ));
            }
            a / b
        }
        BinOp::FloorDiv => {
            if b.numer().sign() == num_bigint::Sign::NoSign {
                return Some(Err(
                    InterpreterError::Runtime("Fraction division by zero".into()).into()
                ));
            }
            (a / b).floor()
        }
        // Mod / Pow on Fraction: uncommon; leave unsupported for now.
        BinOp::Mod | BinOp::Pow => return None,
    };
    Some(Ok(Value::Fraction(Box::new(result))))
}

fn fraction_get_attr(value: &Value, name: &str) -> EvalResult {
    let Value::Fraction(f) = value else { unreachable!("fraction_get_attr only on FRACTION_TYPE") };
    match name {
        // CPython exposes `.numerator` / `.denominator` as the two
        // canonical accessors; BigRational stores them with the sign
        // normalised to the numerator already.
        "numerator" => Ok(bigint_to_value(f.numer())),
        "denominator" => Ok(bigint_to_value(f.denom())),
        _ => Err(InterpreterError::AttributeError(format!(
            "'Fraction' object has no attribute '{name}'"
        ))
        .into()),
    }
}

/// Convert a `BigInt` to a `Value`, falling back to `Value::Float` when
/// the value exceeds the i64 range. Used by `fraction_get_attr` to
/// surface `.numerator` / `.denominator`; matches CPython's
/// `float(Fraction)` lossy semantics past 2^53.
fn bigint_to_value(value: &num_bigint::BigInt) -> Value {
    use num_traits::ToPrimitive as _;
    value.to_i64().map_or_else(|| value.to_f64().map_or(Value::None, Value::Float), Value::Int)
}

// ---------------------------------------------------------------------------
// Datetime cluster slot implementations (Date / DateTime / Time /
// TimeDelta / TimeZone)
// ---------------------------------------------------------------------------
//
// Arithmetic is a cross-type matrix (date + timedelta, datetime -
// datetime, timedelta * int, etc.) so every datetime-cluster slot
// delegates to the same shared `datetime::try_arith` body — each slot
// fn is just a typed entry point that the dispatch layer can reach via
// the per-variant TypeObject. Attribute access (`.year`, `.month`,
// `.hour`, `.seconds`, ...) similarly delegates to the per-variant
// fns in the `datetime` module.

const fn binop_to_sym(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::FloorDiv => "//",
        _ => "",
    }
}

fn datetime_cluster_arith(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    _decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    let sym = binop_to_sym(op);
    if sym.is_empty() {
        return None;
    }
    crate::eval::modules::datetime::try_arith(sym, lhs, rhs)
}

fn date_get_attr(value: &Value, name: &str) -> EvalResult {
    let Value::Date(d) = value else { unreachable!("date_get_attr only on DATE_TYPE") };
    crate::eval::modules::datetime::date_attribute(*d, name)
}

fn datetime_get_attr(value: &Value, name: &str) -> EvalResult {
    let Value::DateTime { dt, tz_offset_secs } = value else {
        unreachable!("datetime_get_attr only on DATETIME_TYPE")
    };
    crate::eval::modules::datetime::datetime_attribute(*dt, *tz_offset_secs, name)
}

fn time_get_attr(value: &Value, name: &str) -> EvalResult {
    let Value::Time(t) = value else { unreachable!("time_get_attr only on TIME_TYPE") };
    crate::eval::modules::datetime::time_attribute(*t, name)
}

fn timedelta_get_attr(value: &Value, name: &str) -> EvalResult {
    let Value::TimeDelta(micros) = value else {
        unreachable!("timedelta_get_attr only on TIMEDELTA_TYPE")
    };
    crate::eval::modules::datetime::timedelta_attribute(*micros, name)
}

// ---------------------------------------------------------------------------
// HashDigest + EnumMember slot implementations (Pass 2c)
// ---------------------------------------------------------------------------

fn hashdigest_get_attr(value: &Value, name: &str) -> EvalResult {
    let Value::HashDigest { algo, bytes } = value else {
        unreachable!("hashdigest_get_attr only on HASHDIGEST_TYPE")
    };
    crate::eval::modules::hashlib::hash_attribute(algo, bytes, name)
}

fn enummember_get_attr(value: &Value, name: &str) -> EvalResult {
    let Value::EnumMember { class_name, member_name, value: inner, .. } = value else {
        unreachable!("enummember_get_attr only on ENUMMEMBER_TYPE")
    };
    match name {
        "name" => Ok(Value::String(member_name.clone().into())),
        "value" => Ok((**inner).clone()),
        _ => Err(InterpreterError::AttributeError(format!(
            "'{class_name}.{member_name}' enum member has no attribute '{name}'"
        ))
        .into()),
    }
}

// ---------------------------------------------------------------------------

/// Construct a `TypeError` for an unsupported comparison between two types.
/// Mirrors CPython's message wording so user-visible errors stay stable
/// across the dispatch migration.
pub fn type_error_unsupported(op: &str, lhs: &Value, rhs: &Value) -> EvalError {
    InterpreterError::TypeError(format!(
        "'{op}' not supported between instances of '{}' and '{}'",
        type_of(lhs).name,
        type_of(rhs).name,
    ))
    .into()
}
