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
    /// materializes the iterable into a `Vec<Value>` for now. Lazy iter
    /// support (with a proper `Value::Iterator` variant + state) is
    /// tracked by `gap-lazy-iterator-value-variant`; the public
    /// `iter()`/`next()` builtins already exist over the eager model.
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
    /// Method-table marker: when true, `method_dispatch` has a
    /// per-type handler in its fn-pointer table (see
    /// `methods_handler_for`). Kept as a bool rather than an fn pointer
    /// so `TypeObject` stays free of the `eval::functions` dependency
    /// cycle.
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
pub type HashSlot = fn(value: &Value) -> Result<i64, EvalError>;

/// Function-pointer shape for the less-than slot. Mirrors `EqSlot`.
/// `<` slot: `None` when this type does not order against `rhs` (so
/// `dispatch_lt` tries the reflected slot, then raises `TypeError`);
/// `Some(Ok(_))` decides the comparison; `Some(Err(_))` propagates an error
/// raised while comparing (e.g. an uncomparable nested list/tuple element).
pub type LtSlot = fn(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>>;

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
    // Builtin dunder methods (`[].__iter__`, `(5).__add__`, `"x".__len__`)
    // resolve to a bound method-wrapper when CPython's type defines the dunder,
    // so `hasattr(x, "__iter__")` / `getattr(x, "__len__")` match. Uniform
    // across builtin types; user Instance/Class resolution is handled by the
    // caller and excluded by `builtin_dunder_present`. Security-blocked dunders
    // never reach here (callers run `validate_attribute` first).
    if is_dunder_name(name) && builtin_dunder_present(value, name) {
        return Ok(Some(bound_method(value, name)));
    }
    // Generator-iterator protocol methods (`send`/`throw`/`close`) on a lazy
    // iterator — a generator / genexp exposes them, so `hasattr(g, "send")` and
    // `g.close` match CPython. `__next__`/`__iter__` come through the dunder path.
    if matches!(value, Value::Generator { .. } | Value::Lazy { .. } | Value::BuiltinIter { .. })
        && matches!(name, "send" | "throw" | "close")
    {
        return Ok(Some(bound_method(value, name)));
    }
    type_of(value).get_attr_slot.map_or_else(|| Ok(None), |slot| slot(value, name).map(Some))
}

/// A `__dunder__` identifier: two leading and two trailing underscores.
fn is_dunder_name(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

/// Dunders present on every object (from CPython 3.12 `dir(object)`), minus the
/// security-blocked ones. `__hash__` is here too: unhashable builtins
/// (list/dict/set/bytearray) still *have* the attribute (it is `None`), so
/// `hasattr([], "__hash__")` is `True`.
const COMMON_DUNDERS: &[&str] = &[
    "__delattr__",
    "__dir__",
    "__doc__",
    "__eq__",
    "__format__",
    "__ge__",
    "__getattribute__",
    "__getstate__",
    "__gt__",
    "__hash__",
    "__init__",
    "__init_subclass__",
    "__le__",
    "__lt__",
    "__ne__",
    "__new__",
    "__reduce__",
    "__reduce_ex__",
    "__repr__",
    "__setattr__",
    "__sizeof__",
    "__str__",
    "__subclasshook__",
];

// Per-type extra dunders (beyond COMMON), transcribed from CPython 3.12
// `dir(type)`. `bool` shares `int`'s set (it subclasses int).
const INT_DUNDERS: &[&str] = &[
    "__abs__",
    "__add__",
    "__and__",
    "__bool__",
    "__ceil__",
    "__divmod__",
    "__float__",
    "__floor__",
    "__floordiv__",
    "__getnewargs__",
    "__index__",
    "__int__",
    "__invert__",
    "__lshift__",
    "__mod__",
    "__mul__",
    "__neg__",
    "__or__",
    "__pos__",
    "__pow__",
    "__radd__",
    "__rand__",
    "__rdivmod__",
    "__rfloordiv__",
    "__rlshift__",
    "__rmod__",
    "__rmul__",
    "__ror__",
    "__round__",
    "__rpow__",
    "__rrshift__",
    "__rshift__",
    "__rsub__",
    "__rtruediv__",
    "__rxor__",
    "__sub__",
    "__truediv__",
    "__trunc__",
    "__xor__",
];
const FLOAT_DUNDERS: &[&str] = &[
    "__abs__",
    "__add__",
    "__bool__",
    "__ceil__",
    "__divmod__",
    "__float__",
    "__floor__",
    "__floordiv__",
    "__getformat__",
    "__getnewargs__",
    "__int__",
    "__mod__",
    "__mul__",
    "__neg__",
    "__pos__",
    "__pow__",
    "__radd__",
    "__rdivmod__",
    "__rfloordiv__",
    "__rmod__",
    "__rmul__",
    "__round__",
    "__rpow__",
    "__rsub__",
    "__rtruediv__",
    "__sub__",
    "__truediv__",
    "__trunc__",
];
const COMPLEX_DUNDERS: &[&str] = &[
    "__abs__",
    "__add__",
    "__bool__",
    "__complex__",
    "__getnewargs__",
    "__mul__",
    "__neg__",
    "__pos__",
    "__pow__",
    "__radd__",
    "__rmul__",
    "__rpow__",
    "__rsub__",
    "__rtruediv__",
    "__sub__",
    "__truediv__",
];
const STR_DUNDERS: &[&str] = &[
    "__add__",
    "__contains__",
    "__getitem__",
    "__getnewargs__",
    "__iter__",
    "__len__",
    "__mod__",
    "__mul__",
    "__rmod__",
    "__rmul__",
];
const BYTES_DUNDERS: &[&str] = &[
    "__add__",
    "__buffer__",
    "__bytes__",
    "__contains__",
    "__getitem__",
    "__getnewargs__",
    "__iter__",
    "__len__",
    "__mod__",
    "__mul__",
    "__rmod__",
    "__rmul__",
];
const BYTEARRAY_DUNDERS: &[&str] = &[
    "__add__",
    "__alloc__",
    "__buffer__",
    "__contains__",
    "__delitem__",
    "__getitem__",
    "__iadd__",
    "__imul__",
    "__iter__",
    "__len__",
    "__mod__",
    "__mul__",
    "__release_buffer__",
    "__rmod__",
    "__rmul__",
    "__setitem__",
];
const LIST_DUNDERS: &[&str] = &[
    "__add__",
    "__class_getitem__",
    "__contains__",
    "__delitem__",
    "__getitem__",
    "__iadd__",
    "__imul__",
    "__iter__",
    "__len__",
    "__mul__",
    "__reversed__",
    "__rmul__",
    "__setitem__",
];
const TUPLE_DUNDERS: &[&str] = &[
    "__add__",
    "__class_getitem__",
    "__contains__",
    "__getitem__",
    "__getnewargs__",
    "__iter__",
    "__len__",
    "__mul__",
    "__rmul__",
];
const DICT_DUNDERS: &[&str] = &[
    "__class_getitem__",
    "__contains__",
    "__delitem__",
    "__getitem__",
    "__ior__",
    "__iter__",
    "__len__",
    "__or__",
    "__reversed__",
    "__ror__",
    "__setitem__",
];
const SET_DUNDERS: &[&str] = &[
    "__and__",
    "__class_getitem__",
    "__contains__",
    "__iand__",
    "__ior__",
    "__isub__",
    "__iter__",
    "__ixor__",
    "__len__",
    "__or__",
    "__rand__",
    "__ror__",
    "__rsub__",
    "__rxor__",
    "__sub__",
    "__xor__",
];
const FROZENSET_DUNDERS: &[&str] = &[
    "__and__",
    "__class_getitem__",
    "__contains__",
    "__iter__",
    "__len__",
    "__or__",
    "__rand__",
    "__ror__",
    "__rsub__",
    "__rxor__",
    "__sub__",
    "__xor__",
];
const RANGE_DUNDERS: &[&str] =
    &["__bool__", "__contains__", "__getitem__", "__iter__", "__len__", "__reversed__"];
const NONE_DUNDERS: &[&str] = &["__bool__"];
// Every builtin iterator/generator (`iter([])`, `(x for x in ...)`), which in
// this engine are `Value::Lazy` / `Value::Generator` / `Value::BuiltinIter`.
const ITER_DUNDERS: &[&str] = &["__iter__", "__length_hint__", "__next__", "__setstate__"];

/// A `@classmethod`/`@staticmethod` that CPython also exposes on *instances*
/// (`{}.fromkeys(...)`, `b"".fromhex(...)`, `b"".maketrans(...)`). Returns the
/// `(type_name, method)` to route through the existing `BuiltinTypeMethod`
/// dispatch (which ignores the receiver). Keeps instance and type forms in sync.
pub(crate) fn instance_classmethod(
    value: &Value,
    method: &str,
) -> Option<(&'static str, &'static str)> {
    match (value, method) {
        (Value::Dict(_), "fromkeys") => Some(("dict", "fromkeys")),
        (Value::Bytes(_), "fromhex") => Some(("bytes", "fromhex")),
        (Value::Bytes(_), "maketrans") => Some(("bytes", "maketrans")),
        (Value::ByteArray(_), "fromhex") => Some(("bytearray", "fromhex")),
        (Value::ByteArray(_), "maketrans") => Some(("bytearray", "maketrans")),
        _ => None,
    }
}

/// The sorted attribute list `dir(value)` returns for a builtin value:
/// `object`'s dunders, the type's own dunders, its callable methods, and its
/// data attributes (`int.real`, `range.start`, …). `None` for types not
/// modelled here — the `dir` builtin only supports builtin *values* (listing
/// universal, access-gated names leaks nothing); Instance/Class/Module/tool
/// introspection stays blocked.
pub(crate) fn builtin_dir(value: &Value) -> Option<Vec<String>> {
    let (dunders, methods, data): (&[&str], &[&str], &[&str]) = match value {
        Value::Int(_) | Value::BigInt(_) | Value::Bool(_) => {
            (INT_DUNDERS, INT_METHODS, &["denominator", "imag", "numerator", "real"])
        }
        Value::Float(_) => (FLOAT_DUNDERS, FLOAT_METHODS, &["imag", "real"]),
        Value::Complex(_) => (COMPLEX_DUNDERS, COMPLEX_METHODS, &["imag", "real"]),
        Value::String(_) => (STR_DUNDERS, STR_METHODS, &[]),
        Value::Bytes(_) => (BYTES_DUNDERS, BYTES_METHODS, &[]),
        Value::ByteArray(_) => (BYTEARRAY_DUNDERS, BYTEARRAY_METHODS, &[]),
        Value::List(_) => (LIST_DUNDERS, LIST_METHODS, &[]),
        Value::Tuple(_) => (TUPLE_DUNDERS, TUPLE_METHODS, &[]),
        Value::Dict(_) => (DICT_DUNDERS, DICT_METHODS, &[]),
        Value::Set(_) => (SET_DUNDERS, SET_METHODS, &[]),
        Value::Frozenset(_) => (FROZENSET_DUNDERS, FROZENSET_METHODS, &[]),
        Value::Range { .. } => (RANGE_DUNDERS, RANGE_METHODS, &["start", "step", "stop"]),
        Value::None => (NONE_DUNDERS, &[], &[]),
        _ => return None,
    };
    // `__class__` is listed by CPython's `dir` on every object. It is a universal
    // attribute: reading it aliases `type(x)` (resolved by
    // `eval::names::resolve_object_attr`), so listing it here matches CPython and
    // grants nothing the `type()` builtin didn't already. Its *write* stays
    // blocked via `validate_attribute`. Kept OUT of the per-type presence tables
    // because it's universal, not type-specific.
    let mut all: Vec<String> = COMMON_DUNDERS
        .iter()
        .chain(dunders)
        .chain(methods)
        .chain(data)
        .copied()
        .chain(std::iter::once("__class__"))
        .map(str::to_string)
        .collect();
    all.sort_unstable();
    all.dedup();
    Some(all)
}

/// Whether CPython's `type(value)` defines dunder `name`, for `hasattr` /
/// `getattr` on builtin values. Returns `false` for user `Instance` / `Class`
/// values (their dunders resolve through the class registry) and any type not
/// modelled here, so the caller falls back to its existing behaviour.
pub(crate) fn builtin_dunder_present(value: &Value, name: &str) -> bool {
    let extras: &[&str] = match value {
        Value::Int(_) | Value::BigInt(_) | Value::Bool(_) => INT_DUNDERS,
        Value::Float(_) => FLOAT_DUNDERS,
        Value::Complex(_) => COMPLEX_DUNDERS,
        Value::String(_) => STR_DUNDERS,
        Value::Bytes(_) => BYTES_DUNDERS,
        Value::ByteArray(_) => BYTEARRAY_DUNDERS,
        Value::List(_) => LIST_DUNDERS,
        Value::Tuple(_) => TUPLE_DUNDERS,
        Value::Dict(_) => DICT_DUNDERS,
        Value::Set(_) => SET_DUNDERS,
        Value::Frozenset(_) => FROZENSET_DUNDERS,
        Value::Range { .. } => RANGE_DUNDERS,
        Value::None => NONE_DUNDERS,
        Value::Lazy { .. } | Value::Generator { .. } | Value::BuiltinIter { .. } => ITER_DUNDERS,
        _ => return false,
    };
    COMMON_DUNDERS.contains(&name) || extras.contains(&name)
}

/// Whether the builtin TYPE object named `type_name` exposes attribute `attr`
/// — the basis for `hasattr(str, "upper")` / `getattr(float, "__format__")` and
/// for rejecting `str.fakemethod`. Mirrors [`builtin_dir`] over the same static
/// tables, so getattr/hasattr on a type object agree with `dir` and with
/// instance attribute access. `__name__`/`__qualname__`/`__call__` are the type
/// object's own attributes. Blocked type-object dunders (`__mro__`, `__dict__`,
/// ...) are intentionally excluded — `validate_attribute` denies them upstream.
pub(crate) fn builtin_type_attr_present(type_name: &str, attr: &str) -> bool {
    if matches!(attr, "__name__" | "__qualname__" | "__call__") {
        return true;
    }
    let (dunders, methods, data): (&[&str], &[&str], &[&str]) = match type_name {
        "int" | "bool" => (INT_DUNDERS, INT_METHODS, &["denominator", "imag", "numerator", "real"]),
        "float" => (FLOAT_DUNDERS, FLOAT_METHODS, &["imag", "real"]),
        "complex" => (COMPLEX_DUNDERS, COMPLEX_METHODS, &["imag", "real"]),
        "str" => (STR_DUNDERS, STR_METHODS, &[]),
        "bytes" => (BYTES_DUNDERS, BYTES_METHODS, &[]),
        "bytearray" => (BYTEARRAY_DUNDERS, BYTEARRAY_METHODS, &[]),
        "list" => (LIST_DUNDERS, LIST_METHODS, &[]),
        "tuple" => (TUPLE_DUNDERS, TUPLE_METHODS, &[]),
        "dict" => (DICT_DUNDERS, DICT_METHODS, &[]),
        "set" => (SET_DUNDERS, SET_METHODS, &[]),
        "frozenset" => (FROZENSET_DUNDERS, FROZENSET_METHODS, &[]),
        "range" => (RANGE_DUNDERS, RANGE_METHODS, &["start", "step", "stop"]),
        "NoneType" => (NONE_DUNDERS, &[], &[]),
        // `object` carries only the universal dunders (checked below).
        "object" => (&[], &[], &[]),
        _ => return false,
    };
    COMMON_DUNDERS.contains(&attr)
        || dunders.contains(&attr)
        || methods.contains(&attr)
        || data.contains(&attr)
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
    // An array indexes exactly like a list (int index → element, slice → a new
    // array of the same typecode); reuse the list path over the shared handle.
    if let Value::Array { typecode, items } = container {
        let result = dispatch_getitem(&Value::List(items.clone()), index)?;
        return Ok(match result {
            Value::List(l) => Value::Array { typecode: *typecode, items: l },
            elem => elem,
        });
    }
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
    // An array assigns exactly like a list over its shared handle (mirroring
    // `dispatch_getitem`); the typecode is preserved by mutating in place.
    if let Value::Array { items, .. } = container {
        let mut list = Value::List(items.clone());
        return dispatch_setitem(&mut list, index, value);
    }
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
    if let Value::Array { items, .. } = value {
        return Ok(items.lock().len());
    }
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
    // A `Lazy` (generator expression / materialised generator) is consumable by
    // the sync iteration path too. This returns all buffered items regardless
    // of the one-shot cursor — the cursor is only advanced by the async
    // `op::iter` / `next` paths — which is exact for a fresh generator (the
    // common case: `"".join(x for x in ...)`); a partially-consumed one
    // re-yields from the start here, a rare, documented divergence.
    if let Value::Lazy { items, .. } = value {
        return Ok(items.clone());
    }
    if let Value::Array { items, .. } = value {
        return Ok(items.lock().clone());
    }
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
    // `array` concatenation / repetition (it has no migrated TypeObject slot).
    if matches!(lhs, Value::Array { .. }) {
        if let Some(result) = array_arith(op, lhs, rhs) {
            return result;
        }
    }
    let lhs_type = type_of(lhs);
    if let Some(result) = (lhs_type.arith_slot)(op, lhs, rhs, decimal_prec) {
        return result;
    }
    let rhs_type = type_of(rhs);
    if let Some(result) = (rhs_type.arith_slot)(op, lhs, rhs, decimal_prec) {
        return result;
    }
    // CPython gives a sequence-specific message when the left operand is a
    // sequence being concatenated with a non-matching type, rather than the
    // generic "unsupported operand type(s)".
    if matches!(op, BinOp::Add) {
        match lhs {
            Value::String(_) | Value::List(_) | Value::Tuple(_) => {
                return Err(InterpreterError::TypeError(format!(
                    "can only concatenate {0} (not \"{1}\") to {0}",
                    lhs_type.name,
                    rhs.python_type_name(),
                ))
                .into());
            }
            Value::Bytes(_) | Value::ByteArray(_) => {
                return Err(InterpreterError::TypeError(format!(
                    "can't concat {} to {}",
                    rhs.python_type_name(),
                    lhs_type.name,
                ))
                .into());
            }
            _ => {}
        }
    }
    // A sequence multiplied by a non-int gets CPython's "can't multiply sequence
    // by non-int of type 'X'" — the valid sequence*int case is handled by the
    // arith slot, so reaching here with a sequence operand means the other is
    // not an int.
    if matches!(op, BinOp::Mul) {
        let is_seq = |v: &Value| {
            matches!(
                v,
                Value::String(_)
                    | Value::List(_)
                    | Value::Tuple(_)
                    | Value::Bytes(_)
                    | Value::ByteArray(_)
            )
        };
        if is_seq(lhs) {
            return Err(InterpreterError::TypeError(format!(
                "can't multiply sequence by non-int of type '{}'",
                rhs.python_type_name(),
            ))
            .into());
        }
        if is_seq(rhs) {
            return Err(InterpreterError::TypeError(format!(
                "can't multiply sequence by non-int of type '{}'",
                lhs.python_type_name(),
            ))
            .into());
        }
    }
    // Use the dynamic class name for instances (`type_of` bottoms out at the
    // static "object" TypeObject), matching CPython's per-class wording.
    Err(InterpreterError::TypeError(format!(
        "unsupported operand type(s) for {}: '{}' and '{}'",
        op.symbol(),
        lhs.python_type_name(),
        rhs.python_type_name(),
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
        return result;
    }
    let rhs_type = type_of(rhs);
    if let Some(result) = (rhs_type.lt_slot)(lhs, rhs) {
        return result;
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
    // An array tests membership exactly like a list over its shared handle
    // (mirroring `dispatch_getitem`/`dispatch_setitem`).
    if let Value::Array { items, .. } = container {
        return dispatch_contains(&Value::List(items.clone()), item);
    }
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
    // An IntEnum / IntFlag / StrEnum member hashes exactly as its underlying
    // int / str (`hash(P.HIGH) == hash(10)`), so dispatch on the inner value's
    // type slot rather than the generic enum fallback hash.
    if let Value::EnumMember {
        value: inner,
        kind:
            crate::value::EnumKind::Int | crate::value::EnumKind::IntFlag | crate::value::EnumKind::Str,
        ..
    } = value
    {
        return dispatch_hash(state, inner);
    }
    if let Value::Instance(inst) = value {
        let registered = state.classes.get(&inst.class_name);
        // Does the class (or any MRO ancestor) define this dunder?
        let defines = |dunder: &str| {
            registered.is_some_and(|class| {
                class.mro.iter().any(|anc| {
                    state.classes.get(anc).is_some_and(|c| c.methods.contains_key(dunder))
                })
            })
        };
        // Resolve `__hash__` along the MRO (first definition wins): a method
        // makes the class hashable, an explicit `__hash__ = None` class attribute
        // makes it unhashable (CPython's `Mutable.__hash__ = None` idiom).
        let hash_is_none = registered.is_some_and(|class| {
            class.mro.iter().find_map(|anc| {
                state.classes.get(anc).and_then(|c| {
                    if c.methods.contains_key("__hash__") {
                        Some(false)
                    } else if matches!(c.class_attrs.get("__hash__"), Some(Value::None)) {
                        Some(true)
                    } else {
                        None
                    }
                })
            }) == Some(true)
        });
        let has_hash = defines("__hash__");
        // CPython sets `__hash__ = None` (unhashable) for a default `@dataclass`
        // (eq=True, frozen=False) and for any class that defines `__eq__`
        // without also defining `__hash__`.
        let dataclass_default =
            registered.is_some_and(|c| c.dataclass_fields.is_some()) && !has_hash;
        if hash_is_none || dataclass_default || (defines("__eq__") && !has_hash) {
            return Err(InterpreterError::TypeError(format!(
                "unhashable type: '{}'",
                inst.class_name
            ))
            .into());
        }
        if !has_hash {
            // Default `object.__hash__`: identity, keyed on the shared-fields
            // Arc address (consistent with identity `==` and `id()`). Covers
            // plain user classes and the bare `object()` sentinel, whose class
            // is not registered. A user-defined `__hash__` is honoured on the
            // async `op::hash` path, which intercepts before this sync route.
            use std::sync::Arc;
            return Ok(finalize_hash(Arc::as_ptr(&inst.fields).addr() as i64));
        }
    }
    let type_obj = type_of(value);
    type_obj.hash_slot.map_or_else(
        || Err(InterpreterError::TypeError(format!("unhashable type: '{}'", type_obj.name)).into()),
        |slot| slot(value),
    )
}

/// Dispatch `lhs == rhs` through the type-object layer.
///
/// Tries the left type's slot first; on `NotImplemented` tries the right
/// type's slot; on a second `NotImplemented` returns `Ok(false)` per
/// CPython's "objects of different types compare unequal" default. User-
/// class instances route through their `__eq__` method if defined.
/// The field values of a `collections.namedtuple` instance in declaration
/// order, or `None` if `inst`'s class is not a namedtuple (no `_fields`).
fn namedtuple_field_values(
    state: &InterpreterState,
    inst: &crate::value::InstanceValue,
) -> Option<Vec<Value>> {
    let class = state.classes.get(&inst.class_name)?;
    let Value::Tuple(field_names) = class.class_attrs.get("_fields")? else {
        return None;
    };
    let fields = inst.fields.lock();
    Some(
        field_names
            .iter()
            .map(|name| match name {
                Value::String(n) => fields.get(n.as_str()).cloned().unwrap_or(Value::None),
                _ => Value::None,
            })
            .collect(),
    )
}

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
                            // `d == d` shares one `SharedFields` Arc — locking it
                            // twice would deadlock, so short-circuit identity.
                            if std::sync::Arc::ptr_eq(&inst.fields, &other_inst.fields) {
                                return Ok(Value::Bool(true));
                            }
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
        // A `collections.namedtuple` subclasses `tuple`, so it compares by
        // value: its field tuple equals another namedtuple's (or a plain
        // tuple's) elements. Detected by the `_fields` class attribute.
        if let Some(lhs_fields) = namedtuple_field_values(state, inst) {
            let rhs_elems = match rhs {
                Value::Tuple(items) => Some(items.clone()),
                Value::Instance(other) => namedtuple_field_values(state, other),
                _ => None,
            };
            if let Some(rhs_elems) = rhs_elems {
                if lhs_fields.len() != rhs_elems.len() {
                    return Ok(Value::Bool(false));
                }
                for (a, b) in lhs_fields.iter().zip(&rhs_elems) {
                    if !matches!(dispatch_eq(state, a, b)?, Value::Bool(true)) {
                        return Ok(Value::Bool(false));
                    }
                }
                return Ok(Value::Bool(true));
            }
            return Ok(Value::Bool(false));
        }
        // User-defined `__eq__` is dispatched at the async eval-layer
        // entry (`eval_compare`), not here. Sync `dispatch_eq` is
        // reached only after the async path declined to short-circuit
        // — at which point the class has no `__eq__` or we're in a
        // context where method dispatch isn't possible (hash, set
        // membership). Identity fallback matches CPython's default.
        //
        // Identity is `Arc::ptr_eq` on the shared field storage — NOT
        // `std::ptr::eq(inst, other_inst)`, which compares the addresses of two
        // separately-cloned `InstanceValue` structs (each `Value::Instance` clone
        // is a fresh struct sharing the same `Arc<fields>`) and is therefore
        // false for every pair, including true aliases. This is the same identity
        // the unified `is` and structural-eq paths use.
        return Ok(Value::Bool(matches!(
            rhs,
            Value::Instance(other_inst) if std::sync::Arc::ptr_eq(&inst.fields, &other_inst.fields)
        )));
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
        Value::Complex(_) => &COMPLEX_TYPE,
        Value::String(_) => &STR_TYPE,
        Value::Bytes(_) => &BYTES_TYPE,
        Value::ByteArray(_) => &BYTEARRAY_TYPE,
        Value::MemoryView(_) => &MEMORYVIEW_TYPE,
        Value::List(_) => &LIST_TYPE,
        Value::Tuple(_) => &TUPLE_TYPE,
        Value::Dict(_) => &DICT_TYPE,
        Value::OrderedDict(_) => &ORDEREDDICT_TYPE,
        Value::Set(_) => &SET_TYPE,
        Value::Frozenset(_) => &FROZENSET_TYPE,
        Value::Range { .. } => &RANGE_TYPE,
        Value::Counter(_) => &COUNTER_TYPE,
        Value::Deque { .. } => &DEQUE_TYPE,
        Value::DefaultDict { .. } => &DEFAULTDICT_TYPE,
        Value::ChainMap(_) => &CHAINMAP_TYPE,
        Value::DictView { .. } => &DICTVIEW_TYPE,
        Value::Decimal(..) => &DECIMAL_TYPE,
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
    get_attr_slot: Some(bool_get_attr),
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
    get_attr_slot: Some(int_get_attr),
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
    get_attr_slot: Some(float_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
static COMPLEX_TYPE: TypeObject = TypeObject {
    name: "complex",
    eq_slot: complex_eq,
    hash_slot: Some(complex_hash_slot),
    // Ordering is a TypeError for complex; `noimpl_lt` -> None -> the dispatcher
    // raises "unsupported operand type(s) for <".
    lt_slot: noimpl_lt,
    contains_slot: None,
    arith_slot: complex_arith,
    iter_slot: None,
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: None,
    // `.real`/`.imag` are attributes; `.conjugate()` is dispatched via the
    // method table (has_methods_table below).
    get_attr_slot: Some(complex_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
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
    contains_slot: Some(bytes_contains),
    arith_slot: bytes_arith,
    iter_slot: Some(bytes_iter),
    get_item_slot: Some(bytes_get_item),
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: Some(bytes_len),
    get_attr_slot: Some(bytes_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
/// `bytearray` — mutable sibling of `bytes`. Shares the read slots (which
/// accept either via `bytes_view`) and adds item-write / item-delete slots.
static BYTEARRAY_TYPE: TypeObject = TypeObject {
    name: "bytearray",
    eq_slot: bytes_eq,
    hash_slot: None,
    lt_slot: bytes_lt,
    contains_slot: Some(bytes_contains),
    arith_slot: bytes_arith,
    iter_slot: Some(bytes_iter),
    get_item_slot: Some(bytes_get_item),
    set_item_slot: Some(bytearray_set_item),
    del_item_slot: Some(bytearray_del_item),
    missing_slot: None,
    len_slot: Some(bytes_len),
    get_attr_slot: Some(bytearray_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
/// `memoryview` — a read view; shares the bytes read slots via `bytes_view`,
/// which unwraps the source. No writes, no arithmetic.
static MEMORYVIEW_TYPE: TypeObject = TypeObject {
    name: "memoryview",
    eq_slot: bytes_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: bytes_lt,
    contains_slot: Some(bytes_contains),
    arith_slot: noimpl_arith,
    iter_slot: Some(bytes_iter),
    get_item_slot: Some(bytes_get_item),
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: Some(bytes_len),
    get_attr_slot: Some(memoryview_get_attr),
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
// `collections.OrderedDict` reuses every dict slot (all now variant-agnostic
// via `Value::as_dict`); only the name and the order-sensitive `eq_slot`
// differ from `DICT_TYPE`.
static ORDEREDDICT_TYPE: TypeObject = TypeObject {
    name: "OrderedDict",
    eq_slot: ordered_dict_eq,
    hash_slot: None,
    lt_slot: noimpl_lt,
    contains_slot: Some(dict_contains),
    arith_slot: noimpl_arith,
    iter_slot: Some(dict_iter),
    get_item_slot: Some(dict_get_item),
    set_item_slot: Some(dict_set_item),
    del_item_slot: Some(dict_del_item),
    missing_slot: None,
    len_slot: Some(dict_len),
    get_attr_slot: Some(dict_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};
static SET_TYPE: TypeObject = TypeObject {
    name: "set",
    eq_slot: set_eq,
    hash_slot: None,
    lt_slot: set_lt,
    contains_slot: Some(sequence_contains),
    arith_slot: set_arith,
    iter_slot: Some(set_iter),
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
/// `frozenset` — the immutable, hashable sibling of `set`. Shares the set's
/// equality, membership, iteration, length, and algebra slots (all of which
/// accept either concrete type), but adds a real `hash_slot` and exposes only
/// the non-mutating methods via `frozenset_get_attr`.
static FROZENSET_TYPE: TypeObject = TypeObject {
    name: "frozenset",
    eq_slot: set_eq,
    hash_slot: Some(fallback_hash_slot),
    lt_slot: set_lt,
    contains_slot: Some(sequence_contains),
    arith_slot: set_arith,
    iter_slot: Some(set_iter),
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: Some(sequence_len),
    get_attr_slot: Some(frozenset_get_attr),
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
    get_attr_slot: Some(range_get_attr),
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
    eq_slot: deque_eq,
    hash_slot: None,
    lt_slot: noimpl_lt,
    contains_slot: Some(deque_contains),
    arith_slot: noimpl_arith,
    iter_slot: Some(deque_iter),
    get_item_slot: Some(deque_get_item),
    set_item_slot: Some(deque_set_item),
    del_item_slot: Some(deque_del_item),
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
/// Live `dict_keys` / `dict_values` / `dict_items` view. Iteration,
/// `len`, and membership read the shared dict on demand; the set
/// operators on keys/items are handled by `apply_binop`'s coercion.
static DICTVIEW_TYPE: TypeObject = TypeObject {
    name: "dict_view",
    eq_slot: dictview_eq,
    hash_slot: None,
    lt_slot: dictview_lt,
    contains_slot: Some(dictview_contains),
    arith_slot: noimpl_arith,
    iter_slot: Some(dictview_iter),
    get_item_slot: None,
    set_item_slot: None,
    del_item_slot: None,
    missing_slot: None,
    len_slot: Some(dictview_len),
    get_attr_slot: None,
    set_attr_slot: None,
    has_methods_table: true,
};

#[expect(clippy::unnecessary_wraps, reason = "IterSlot protocol")]
fn dictview_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    let Value::DictView { dict, kind } = value else {
        unreachable!("dictview_iter only on DICTVIEW_TYPE")
    };
    let guard = dict.lock();
    Ok(match kind {
        crate::value::DictViewKind::Keys => {
            guard.keys().map(crate::value::ValueKey::to_value).collect()
        }
        crate::value::DictViewKind::Values => guard.values().cloned().collect(),
        crate::value::DictViewKind::Items => {
            guard.iter().map(|(k, v)| Value::Tuple(vec![k.to_value(), v.clone()])).collect()
        }
    })
}

#[expect(clippy::unnecessary_wraps, reason = "LenSlot protocol")]
fn dictview_len(value: &Value) -> Result<usize, EvalError> {
    let Value::DictView { dict, .. } = value else {
        unreachable!("dictview_len only on DICTVIEW_TYPE")
    };
    let len = dict.lock().len();
    Ok(len)
}

fn dictview_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let Value::DictView { dict, kind } = container else {
        unreachable!("dictview_contains only on DICTVIEW_TYPE")
    };
    let guard = dict.lock();
    Ok(match kind {
        crate::value::DictViewKind::Keys => {
            crate::eval::literals::value_to_key(item).is_ok_and(|k| guard.contains_key(&k))
        }
        crate::value::DictViewKind::Values => {
            guard.values().any(|v| crate::eval::operations::values_equal_pub(v, item))
        }
        crate::value::DictViewKind::Items => match item {
            Value::Tuple(pair) if pair.len() == 2 => crate::eval::literals::value_to_key(&pair[0])
                .ok()
                .and_then(|k| guard.get(&k))
                .is_some_and(|v| crate::eval::operations::values_equal_pub(v, &pair[1])),
            _ => false,
        },
    })
}

fn dictview_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    // A view compares equal to another view / set / frozenset with the
    // same elements (CPython: keys/items views are set-like).
    let to_elems = |v: &Value| -> Option<Vec<Value>> {
        match v {
            Value::DictView { .. } => dictview_iter(v).ok(),
            _ => v.set_items(),
        }
    };
    let (a, b) = (to_elems(lhs)?, to_elems(rhs)?);
    Some(a.len() == b.len() && a.iter().all(|x| b.iter().any(|y| recurse_eq(x, y))))
}

/// Proper-subset `<` for a set-like dict view against another view / set /
/// frozenset (CPython: keys/items views support the set ordering operators).
/// `<=`/`>`/`>=` derive from this plus `dictview_eq` in `compare_builtin`.
fn dictview_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    let to_elems = |v: &Value| -> Option<Vec<Value>> {
        match v {
            Value::DictView { .. } => dictview_iter(v).ok(),
            _ => v.set_items(),
        }
    };
    let (a, b) = (to_elems(lhs)?, to_elems(rhs)?);
    let is_proper = a.len() < b.len() && a.iter().all(|av| b.iter().any(|bv| recurse_eq(av, bv)));
    Some(Ok(is_proper))
}

/// `collections.ChainMap` TypeObject. Lookups/iteration/len search the
/// underlying (shared) maps left-to-right; writes and `del` target the
/// first map. Methods (keys/values/items/get/new_child/…) route through
/// the method-dispatch table.
static CHAINMAP_TYPE: TypeObject = TypeObject {
    name: "ChainMap",
    eq_slot: noimpl_eq,
    hash_slot: None,
    lt_slot: noimpl_lt,
    contains_slot: Some(chainmap_contains),
    arith_slot: noimpl_arith,
    iter_slot: Some(chainmap_iter),
    get_item_slot: Some(chainmap_get_item),
    set_item_slot: Some(chainmap_set_item),
    del_item_slot: Some(chainmap_del_item),
    missing_slot: None,
    len_slot: Some(chainmap_len),
    get_attr_slot: Some(chainmap_get_attr),
    set_attr_slot: None,
    has_methods_table: true,
};

/// Iterate a `ChainMap`'s maps, invoking `f` with each underlying dict's
/// locked contents. Non-dict entries (shouldn't occur) are skipped.
fn chainmap_for_each_map(
    maps: &[Value],
    mut f: impl FnMut(&indexmap::IndexMap<crate::value::ValueKey, Value>),
) {
    for m in maps {
        if let Value::Dict(map) = m {
            f(&map.lock());
        }
    }
}

/// Materialise a `ChainMap`'s effective mapping (reversed-map iteration
/// order, first map's value winning) — used by `dict(chainmap)`.
pub(crate) fn chainmap_contents(
    maps: &[Value],
) -> indexmap::IndexMap<crate::value::ValueKey, Value> {
    let mut out = indexmap::IndexMap::new();
    for m in maps.iter().rev() {
        if let Value::Dict(map) = m {
            for (k, v) in map.lock().iter() {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

fn chainmap_get_item(container: &Value, index: &Value) -> Result<Value, EvalError> {
    let Value::ChainMap(maps) = container else {
        unreachable!("chainmap_get_item only on CHAINMAP_TYPE")
    };
    let key = crate::eval::literals::value_to_key(index)?;
    for m in maps {
        if let Value::Dict(map) = m {
            if let Some(v) = map.lock().get(&key).cloned() {
                return Ok(v);
            }
        }
    }
    Err(crate::value::ExceptionValue::key_error(&key).into())
}

fn chainmap_set_item(
    container: &mut Value,
    index: &Value,
    value: Value,
) -> Result<isize, EvalError> {
    let Value::ChainMap(maps) = container else {
        unreachable!("chainmap_set_item only on CHAINMAP_TYPE")
    };
    let key = crate::eval::literals::value_to_key(index)?;
    // Writes always target the first map (CPython). The map is a shared
    // Dict, so its own store is mutated; size is accounted there.
    if let Some(Value::Dict(first)) = maps.first() {
        first.lock().insert(key, value);
    }
    Ok(0)
}

fn chainmap_del_item(container: &mut Value, index: &Value) -> Result<isize, EvalError> {
    let Value::ChainMap(maps) = container else {
        unreachable!("chainmap_del_item only on CHAINMAP_TYPE")
    };
    let key = crate::eval::literals::value_to_key(index)?;
    if let Some(Value::Dict(first)) = maps.first() {
        if first.lock().shift_remove(&key).is_some() {
            return Ok(0);
        }
    }
    // CPython: "Key not found in the first mapping: <key>".
    Err(crate::value::ExceptionValue::new(
        "KeyError",
        format!("Key not found in the first mapping: {key}"),
    )
    .into())
}

fn chainmap_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let Value::ChainMap(maps) = container else {
        unreachable!("chainmap_contains only on CHAINMAP_TYPE")
    };
    let key = crate::eval::literals::value_to_key(item)?;
    let mut found = false;
    chainmap_for_each_map(maps, |m| found = found || m.contains_key(&key));
    Ok(found)
}

#[expect(clippy::unnecessary_wraps, reason = "LenSlot protocol")]
fn chainmap_len(value: &Value) -> Result<usize, EvalError> {
    let Value::ChainMap(maps) = value else { unreachable!("chainmap_len only on CHAINMAP_TYPE") };
    #[expect(
        clippy::mutable_key_type,
        reason = "ValueKey's interior mutability is not used for its Hash/Eq (keys are hashable \
                  ValueKey variants), so it is a sound HashSet key"
    )]
    let mut seen: rustc_hash::FxHashSet<crate::value::ValueKey> = rustc_hash::FxHashSet::default();
    chainmap_for_each_map(maps, |m| {
        for k in m.keys() {
            seen.insert(k.clone());
        }
    });
    Ok(seen.len())
}

#[expect(clippy::unnecessary_wraps, reason = "IterSlot protocol")]
fn chainmap_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    let Value::ChainMap(maps) = value else { unreachable!("chainmap_iter only on CHAINMAP_TYPE") };
    // CPython iterates `dict.fromkeys(chain.from_iterable(reversed(maps)))`
    // — keys of later maps come first, deduped, first occurrence wins.
    #[expect(
        clippy::mutable_key_type,
        reason = "ValueKey's interior mutability is not used for its Hash/Eq (keys are hashable \
                  ValueKey variants), so it is a sound HashSet key"
    )]
    let mut seen: rustc_hash::FxHashSet<crate::value::ValueKey> = rustc_hash::FxHashSet::default();
    let mut order: Vec<crate::value::ValueKey> = Vec::new();
    for m in maps.iter().rev() {
        if let Value::Dict(map) = m {
            for k in map.lock().keys() {
                if seen.insert(k.clone()) {
                    order.push(k.clone());
                }
            }
        }
    }
    Ok(order.into_iter().map(|k| k.to_value()).collect())
}

/// `ChainMap.maps` / `.parents` attribute access; other names raise.
fn chainmap_get_attr(value: &Value, attr: &str) -> Result<Value, EvalError> {
    let Value::ChainMap(maps) = value else {
        unreachable!("chainmap_get_attr only on CHAINMAP_TYPE")
    };
    match attr {
        // `.maps` is the list of underlying mappings (shared handles).
        "maps" => Ok(Value::List(crate::value::shared_list(maps.clone()))),
        // `.parents` is a new ChainMap over all but the first map.
        "parents" => {
            let rest: Vec<Value> = maps.iter().skip(1).cloned().collect();
            let rest = if rest.is_empty() {
                vec![Value::Dict(crate::value::shared_dict(indexmap::IndexMap::new()))]
            } else {
                rest
            };
            Ok(Value::ChainMap(rest))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'ChainMap' object has no attribute '{attr}'"
        ))
        .into()),
    }
}

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
    contains_slot: Some(enummember_contains),
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
    lt_slot: date_lt,
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
    lt_slot: datetime_lt,
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
    lt_slot: time_lt,
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
    lt_slot: timedelta_lt,
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
    hash_slot: Some(decimal_hash_slot),
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
    hash_slot: Some(fraction_hash_slot),
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
    // still routes contains through the legacy path. Tracked by
    // refactor-typeobject-promote-remaining-variants.
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

/// The byte contents of a `bytes` or `bytearray` value (a copy for the shared
/// bytearray), or `None` for anything else. Lets the bytes slots serve both.
/// The current bytes behind a `bytes`/`bytearray`/`memoryview` value (empty for
/// anything else). Public so the memoryview method dispatch can read the buffer.
#[must_use]
pub fn memoryview_bytes(value: &Value) -> Vec<u8> {
    bytes_view(value).unwrap_or_default()
}

fn bytes_view(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::Bytes(b) => Some(b.clone()),
        Value::ByteArray(b) => Some(b.lock().clone()),
        Value::MemoryView(inner) => bytes_view(inner),
        _ => None,
    }
}

fn bytes_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    Some(bytes_view(lhs)? == bytes_view(rhs)?)
}

fn list_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::List(a) = lhs else { return None };
    let Value::List(b) = rhs else { return None };
    if std::sync::Arc::ptr_eq(a, b) {
        return Some(true);
    }
    // Snapshot under the locks and release before `elementwise_eq` →
    // `recurse_eq`, which re-locks these lists on a self-reference (`a == b`
    // where `a[0] is a`) — holding the lock across it would deadlock.
    let a = a.lock().clone();
    let b = b.lock().clone();
    Some(elementwise_eq(&a, &b))
}

fn tuple_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let Value::Tuple(a) = lhs else { return None };
    let Value::Tuple(b) = rhs else { return None };
    Some(elementwise_eq(a, b))
}

fn dict_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    // Accepts dict or OrderedDict on either side; comparison is unordered
    // (CPython `dict.__eq__`). Order-sensitive OrderedDict/OrderedDict
    // comparison is handled by `ordered_dict_eq`.
    let a = lhs.as_dict()?;
    let b = rhs.as_dict()?;
    if std::sync::Arc::ptr_eq(a, b) {
        return Some(true);
    }
    // Snapshot both under their locks and release before `recurse_eq`,
    // which may lock other dicts — never hold a dict lock across it.
    let a = a.lock().clone();
    let b = b.lock().clone();
    if a.len() != b.len() {
        return Some(false);
    }
    let equal = a.iter().all(|(k, v)| b.get(k).is_some_and(|bv| recurse_eq(v, bv)));
    Some(equal)
}

/// `OrderedDict.__eq__`: order-sensitive when the other operand is also an
/// OrderedDict (CPython compares key/value pairs pairwise in sequence);
/// against a plain dict it falls back to unordered `dict_eq`.
fn ordered_dict_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    if let (Value::OrderedDict(a), Value::OrderedDict(b)) = (lhs, rhs) {
        if std::sync::Arc::ptr_eq(a, b) {
            return Some(true);
        }
        let a = a.lock().clone();
        let b = b.lock().clone();
        if a.len() != b.len() {
            return Some(false);
        }
        let equal =
            a.iter().zip(b.iter()).all(|((ka, va), (kb, vb))| ka == kb && recurse_eq(va, vb));
        return Some(equal);
    }
    dict_eq(lhs, rhs)
}

fn set_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let a = lhs.set_items()?;
    let b = rhs.set_items()?;
    if a.len() != b.len() {
        return Some(false);
    }
    // Set equality is unordered: every element in a must appear in b under
    // the same eq semantics.
    let equal = a.iter().all(|av| b.iter().any(|bv| recurse_eq(av, bv)));
    Some(equal)
}

/// `set`/`frozenset` `<` — proper-subset test (accepts either concrete
/// type on both sides). The `<=`/`>`/`>=` forms derive from this via
/// `compare_builtin` (`<=` = `<` or `==`, `>` = swapped `<`), giving the
/// full subset/superset lattice.
fn set_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    let a = lhs.set_items()?;
    let b = rhs.set_items()?;
    // Proper subset: strictly smaller and every element contained.
    let is_proper = a.len() < b.len() && a.iter().all(|av| b.iter().any(|bv| recurse_eq(av, bv)));
    Some(Ok(is_proper))
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
pub(crate) fn recurse_eq(lhs: &Value, rhs: &Value) -> bool {
    // Bound the structural recursion so comparing two *distinct* cyclic
    // containers stops instead of overflowing the host stack (CPython answers
    // this with RecursionError; a same-object cycle is already caught by the
    // `Arc::ptr_eq` short-circuits, so it never reaches the limit).
    let Some(_depth) = crate::cycle::eq_depth_enter() else {
        return false;
    };
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
const fn noimpl_lt(_lhs: &Value, _rhs: &Value) -> Option<Result<bool, EvalError>> {
    None
}

/// `date < date` — `NaiveDate` is chronologically ordered.
fn date_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    match (lhs, rhs) {
        (Value::Date(a), Value::Date(b)) => Some(Ok(a < b)),
        _ => None,
    }
}

/// `time < time` — `NaiveTime` is ordered within a day.
fn time_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    match (lhs, rhs) {
        (Value::Time(a), Value::Time(b)) => Some(Ok(a < b)),
        _ => None,
    }
}

/// `timedelta < timedelta` — compare the microsecond magnitudes.
fn timedelta_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    match (lhs, rhs) {
        (Value::TimeDelta(a), Value::TimeDelta(b)) => Some(Ok(a < b)),
        _ => None,
    }
}

/// `datetime < datetime` — naive pairs compare wall-clock; aware pairs compare
/// absolute instants (each shifted to UTC by its offset). Mixing a naive and an
/// aware datetime raises, matching CPython.
fn datetime_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    let (
        Value::DateTime { dt: a, tz_offset_secs: ta },
        Value::DateTime { dt: b, tz_offset_secs: tb },
    ) = (lhs, rhs)
    else {
        return None;
    };
    match (ta, tb) {
        (None, None) => Some(Ok(a < b)),
        (Some(oa), Some(ob)) => {
            let ia = *a - chrono::Duration::seconds(i64::from(*oa));
            let ib = *b - chrono::Duration::seconds(i64::from(*ob));
            Some(Ok(ia < ib))
        }
        _ => Some(Err(InterpreterError::TypeError(
            "can't compare offset-naive and offset-aware datetimes".into(),
        )
        .into())),
    }
}

fn bool_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    let Value::Bool(a) = lhs else { return None };
    let av = i64::from(*a);
    match rhs {
        Value::Bool(b) => Some(Ok(av < i64::from(*b))),
        Value::Int(b) => Some(Ok(av < *b)),
        #[expect(
            clippy::cast_precision_loss,
            reason = "Python bool↔float compare matches CPython's lossy compare"
        )]
        Value::Float(b) => Some(Ok((av as f64) < *b)),
        _ => None,
    }
}

fn int_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    let a = crate::value::value_as_bigint(lhs)?;
    match rhs {
        Value::Int(_) | Value::BigInt(_) | Value::Bool(_) => {
            let b = crate::value::value_as_bigint(rhs)?;
            Some(Ok(a < b))
        }
        Value::Float(b) => {
            use num_traits::ToPrimitive as _;
            Some(Ok(a.to_f64().is_some_and(|af| af < *b)))
        }
        _ => None,
    }
}

fn float_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    let Value::Float(a) = lhs else { return None };
    match rhs {
        Value::Float(b) => Some(Ok(a < b)),
        Value::Bool(b) => Some(Ok(*a < if *b { 1.0 } else { 0.0 })),
        #[expect(
            clippy::cast_precision_loss,
            reason = "Python int↔float compare matches CPython's lossy compare"
        )]
        Value::Int(b) => Some(Ok(*a < (*b as f64))),
        Value::BigInt(b) => {
            use num_traits::ToPrimitive as _;
            Some(Ok(b.to_f64().is_some_and(|bf| *a < bf)))
        }
        _ => None,
    }
}

fn str_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    let Value::String(a) = lhs else { return None };
    let Value::String(b) = rhs else { return None };
    Some(Ok(a < b))
}

/// Coerce a numeric value to `Complex64` for complex arithmetic/equality.
/// `None` for a non-numeric operand (so the caller returns NotImplemented).
pub(crate) fn value_to_complex(v: &Value) -> Option<num_complex::Complex64> {
    use num_traits::ToPrimitive as _;
    let re = match v {
        Value::Complex(c) => return Some(**c),
        Value::Float(f) => *f,
        #[expect(clippy::cast_precision_loss, reason = "matches Python complex(int) coercion")]
        Value::Int(i) => *i as f64,
        Value::Bool(b) => f64::from(*b),
        Value::BigInt(b) => b.to_f64()?,
        _ => return None,
    };
    Some(num_complex::Complex64::new(re, 0.0))
}

/// Arithmetic where at least one operand is `complex`: `+ - * / **` coerce the
/// other operand (int/float/bool/BigInt) to complex; `//` and `%` raise
/// TypeError (CPython has no floor/mod for complex); division by zero raises
/// ZeroDivisionError.
fn complex_arith(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    _decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    if !matches!(lhs, Value::Complex(_)) && !matches!(rhs, Value::Complex(_)) {
        return None;
    }
    let a = value_to_complex(lhs)?;
    let b = value_to_complex(rhs)?;
    let out = match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => {
            if b.re == 0.0 && b.im == 0.0 {
                return Some(Err(EvalError::Exception(crate::value::ExceptionValue::new(
                    "ZeroDivisionError",
                    "complex division by zero",
                ))));
            }
            a / b
        }
        // An integer exponent uses exact repeated squaring (`powi`), so
        // `1j**2 == (-1+0j)` exactly rather than carrying float error from the
        // exp/log path; other exponents use the general complex power.
        BinOp::Pow => match rhs {
            Value::Int(n) => i32::try_from(*n).map_or_else(|_| a.powc(b), |e| a.powi(e)),
            Value::Bool(bl) => a.powi(i32::from(*bl)),
            _ => a.powc(b),
        },
        BinOp::FloorDiv | BinOp::Mod => {
            return Some(Err(InterpreterError::TypeError(
                "can't take floor or mod of complex number.".into(),
            )
            .into()));
        }
    };
    Some(Ok(Value::Complex(Box::new(out))))
}

/// `complex` equality: value-equal to another complex or a real number with a
/// zero imaginary part; unequal (not an error) to a non-number.
fn complex_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    if !matches!(lhs, Value::Complex(_)) && !matches!(rhs, Value::Complex(_)) {
        return None;
    }
    match (value_to_complex(lhs), value_to_complex(rhs)) {
        (Some(a), Some(b)) => Some(a == b),
        _ => Some(false),
    }
}

/// CPython's complex hash: `hash(re) + _PyHASH_IMAG * hash(im)` in wrapping
/// `Py_hash_t`. With `im == 0` this reduces to the real part's hash, so
/// `hash(1+0j) == hash(1)` and complex/int/float share dict and set slots.
fn complex_hash_slot(value: &Value) -> Result<i64, EvalError> {
    let Value::Complex(c) = value else { unreachable!("complex_hash_slot sees only Complex") };
    let combined =
        float_hash_impl(c.re).wrapping_add(1_000_003_i64.wrapping_mul(float_hash_impl(c.im)));
    Ok(finalize_hash(combined))
}

fn bytes_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    let (a, b) = (bytes_view(lhs)?, bytes_view(rhs)?);
    Some(Ok(a < b))
}

fn list_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    let Value::List(a) = lhs else { return None };
    let Value::List(b) = rhs else { return None };
    // `l < l` (and the `<=`/`>`/`>=` forms that derive from it) shares one Arc;
    // locking it twice would deadlock. A list is never a proper `<` of itself.
    if std::sync::Arc::ptr_eq(a, b) {
        return Some(Ok(false));
    }
    let a_guard = a.lock();
    let b_guard = b.lock();
    Some(lex_lt(&a_guard, &b_guard))
}

fn tuple_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    let Value::Tuple(a) = lhs else { return None };
    let Value::Tuple(b) = rhs else { return None };
    Some(lex_lt(a, b))
}

/// Lexicographic less-than for list/tuple: first non-equal element decides;
/// if one is a prefix of the other, the shorter is less. Equality recurses
/// through the eq dispatch (so bool↔int unification holds inside lists too).
/// A genuine `TypeError` from the deciding pair (e.g. `2 < "a"`) propagates,
/// matching CPython — it is not swallowed into `false`.
fn lex_lt(a: &[Value], b: &[Value]) -> Result<bool, EvalError> {
    for (x, y) in a.iter().zip(b.iter()) {
        if !recurse_eq(x, y) {
            // First inequal position decides.
            return dispatch_lt(x, y);
        }
    }
    Ok(a.len() < b.len())
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
    // Set/frozenset membership is an O(1) table probe.
    match container {
        Value::Set(b) => return Ok(b.lock().contains(item)),
        Value::Frozenset(b) => return Ok(b.contains(item)),
        _ => {}
    }
    let Value::Tuple(items) = container else {
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
fn dict_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let Some(map) = container.as_dict() else {
        unreachable!("dict_contains only on dict/OrderedDict types")
    };
    // An unhashable probe raises `TypeError: unhashable type`, it does not answer
    // False — propagate the value_to_key error instead of swallowing it.
    let key = crate::eval::literals::value_to_key(item)?;
    Ok(map.lock().contains_key(&key))
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

/// `item in bytes/bytearray/memoryview`. An int (or bool) tests membership of
/// that byte value (raising if outside `range(0, 256)`), a bytes-like tests for
/// a contiguous subsequence (the empty sequence is always present), and any
/// other type raises — matching CPython's `bytes.__contains__`.
fn bytes_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let haystack = bytes_view(container).unwrap_or_default();
    match item {
        Value::Int(_) | Value::Bool(_) => {
            let n = match item {
                Value::Int(i) => *i,
                Value::Bool(b) => i64::from(*b),
                _ => unreachable!(),
            };
            if !(0..=255).contains(&n) {
                return Err(
                    InterpreterError::ValueError("byte must be in range(0, 256)".into()).into()
                );
            }
            Ok(haystack.iter().any(|&b| i64::from(b) == n))
        }
        Value::Bytes(_) | Value::ByteArray(_) | Value::MemoryView(_) => {
            let needle = bytes_view(item).unwrap_or_default();
            Ok(needle.is_empty()
                || haystack.windows(needle.len()).any(|window| window == needle.as_slice()))
        }
        other => Err(InterpreterError::TypeError(format!(
            "a bytes-like object is required, not '{}'",
            other.type_name()
        ))
        .into()),
    }
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
    let (Value::Bytes(_) | Value::ByteArray(_)) = lhs else { return None };
    match op {
        BinOp::Add if matches!(rhs, Value::Bytes(_) | Value::ByteArray(_)) => {
            Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs))
        }
        BinOp::Mul if matches!(rhs, Value::Int(_) | Value::Bool(_)) => {
            Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs))
        }
        // `bytes % args` / `bytearray % args` — printf-style bytes formatting.
        BinOp::Mod => Some(crate::eval::operations::apply_binop_builtin(op, lhs, rhs)),
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

/// `array + array` (concat, matching typecodes) and `array * int` (repetition).
/// Returns `None` for other ops so the caller falls through to its error path.
fn array_arith(op: BinOp, lhs: &Value, rhs: &Value) -> Option<Result<Value, EvalError>> {
    let Value::Array { typecode, items } = lhs else { return None };
    match op {
        BinOp::Add => {
            let Value::Array { typecode: rt, items: ri } = rhs else {
                return Some(Err(InterpreterError::TypeError(format!(
                    "can only append array (not \"{}\") to array",
                    rhs.python_type_name()
                ))
                .into()));
            };
            if rt != typecode {
                return Some(Err(InterpreterError::TypeError(
                    "bad argument type for built-in operation".into(),
                )
                .into()));
            }
            let mut combined = items.lock().clone();
            combined.extend(ri.lock().iter().cloned());
            Some(Ok(Value::Array {
                typecode: *typecode,
                items: crate::value::shared_list(combined),
            }))
        }
        BinOp::Mul => {
            let n = match rhs {
                Value::Int(i) => *i,
                Value::Bool(b) => i64::from(*b),
                _ => return None,
            };
            let src = items.lock().clone();
            let mut out = Vec::new();
            for _ in 0..n.max(0) {
                out.extend(src.iter().cloned());
            }
            Some(Ok(Value::Array { typecode: *typecode, items: crate::value::shared_list(out) }))
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

/// `set - set` (difference). Other set operators (`|`, `&`, `^`) stay on
/// the direct `apply_binop` path with the int bitwise operators.
fn set_arith(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    _decimal_prec: i64,
) -> Option<Result<Value, EvalError>> {
    let (Value::Set(_) | Value::Frozenset(_)) = lhs else { return None };
    match op {
        BinOp::Sub if matches!(rhs, Value::Set(_) | Value::Frozenset(_)) => {
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
/// them. The clone is the load-bearing cost; lazy iterator storage is
/// tracked by `gap-lazy-iterator-value-variant`.
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
    let Value::Tuple(items) = value else {
        unreachable!("sequence_iter only attached to list/tuple TypeObjects")
    };
    Ok(items.clone())
}

/// `iter(set)` / `iter(frozenset)` — yield elements in the set's stored CPython
/// hash-table order.
fn set_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    match value {
        Value::Set(b) => Ok(b.lock().iter_ordered()),
        Value::Frozenset(b) => Ok(b.iter_ordered()),
        _ => unreachable!("set_iter only attached to set/frozenset TypeObjects"),
    }
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
    let b = bytes_view(value).unwrap_or_default();
    Ok(b.iter().map(|&byte| Value::Int(i64::from(byte))).collect())
}

/// `iter(dict)` — yield the keys, matching CPython's dict iteration.
#[expect(clippy::unnecessary_wraps, reason = "IterSlot protocol; dict iteration cannot fail")]
fn dict_iter(value: &Value) -> Result<Vec<Value>, EvalError> {
    let Some(map) = value.as_dict() else {
        unreachable!("dict_iter only on dict/OrderedDict types")
    };
    Ok(map.lock().keys().map(crate::value::ValueKey::to_value).collect())
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

fn none_hash(_value: &Value) -> Result<i64, EvalError> {
    // CPython 3.12 returns a deterministic constant for `hash(None)` (the
    // address of `Py_None`'s singleton, stable across runs). 0 is the
    // platform-independent choice that preserves `hash(None) == hash(None)`
    // and never collides with `hash(0)` after `finalize_hash` (0 stays 0).
    Ok(0)
}

fn bool_hash(value: &Value) -> Result<i64, EvalError> {
    let Value::Bool(b) = value else { unreachable!("bool_hash sees only Value::Bool") };
    // `hash(True) == hash(1)` and `hash(False) == hash(0)` per CPython —
    // bool is a subclass of int, so its hash IS the int hash.
    Ok(finalize_hash(int_hash_impl(i64::from(*b))))
}

fn int_hash_slot(value: &Value) -> Result<i64, EvalError> {
    Ok(match value {
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
    })
}

#[expect(
    clippy::cast_possible_wrap,
    reason = "abs is bounded by HASH_MODULUS (~2^61), well within i64::MAX; the cast is sign-preserving"
)]
const fn int_hash_impl(n: i64) -> i64 {
    let abs = n.unsigned_abs() % HASH_MODULUS;
    if n < 0 { -(abs as i64) } else { abs as i64 }
}

fn float_hash_slot(value: &Value) -> Result<i64, EvalError> {
    let Value::Float(f) = value else { unreachable!("float_hash_slot sees only Value::Float") };
    Ok(finalize_hash(float_hash_impl(*f)))
}

// --- Rational numeric hash (Decimal / Fraction) ---------------------------
// CPython hashes every exact number through the same rational formula so that
// equal values across int/float/Decimal/Fraction share a hash
// (`hash(Decimal('2')) == hash(2) == hash(2.0) == hash(Fraction(2, 1))`).
// HASH_MODULUS (2^61 - 1) is a Mersenne prime, so a modular inverse exists via
// Fermat's little theorem and all arithmetic stays in modular space — no giant
// `10^scale` intermediate that a hostile Decimal exponent could blow up.

fn mulmod(a: u64, b: u64, m: u64) -> u64 {
    ((u128::from(a) * u128::from(b)) % u128::from(m)) as u64
}

fn powmod(base: u64, mut exp: u64, m: u64) -> u64 {
    let mut base = base % m;
    let mut result = 1u64;
    while exp > 0 {
        if exp & 1 == 1 {
            result = mulmod(result, base, m);
        }
        base = mulmod(base, base, m);
        exp >>= 1;
    }
    result
}

/// Assemble CPython's rational hash from the absolute numerator and denominator
/// already reduced modulo `HASH_MODULUS`, applying the numerator's sign.
fn rational_hash(n_abs_mod: u64, d_mod: u64, negative: bool) -> i64 {
    let hash_abs = if d_mod == 0 {
        // Denominator is a multiple of the modulus: CPython yields _PyHASH_INF.
        HASH_INF
    } else {
        // d^(p-2) is the modular inverse of d for prime p (Fermat).
        let d_inv = powmod(d_mod, HASH_MODULUS - 2, HASH_MODULUS);
        mulmod(n_abs_mod, d_inv, HASH_MODULUS) as i64
    };
    finalize_hash(if negative { -hash_abs } else { hash_abs })
}

/// Reduce a `BigInt`'s absolute value modulo `HASH_MODULUS` to a `u64`.
fn bigint_abs_mod(n: &num_bigint::BigInt) -> u64 {
    use num_traits::{Signed as _, ToPrimitive as _};
    (n.abs() % num_bigint::BigInt::from(HASH_MODULUS)).to_u64().unwrap_or(0)
}

fn decimal_hash_slot(value: &Value) -> Result<i64, EvalError> {
    use num_traits::Signed as _;
    let Value::Decimal(d, _) = value else { unreachable!("decimal_hash_slot sees only Decimal") };
    // value == mantissa * 10^(-scale); represent as numerator / denominator and
    // hash the rational. A signed zero hashes like +0 (mantissa is zero).
    let (mantissa, scale) = d.as_bigint_and_exponent();
    let m_mod = bigint_abs_mod(&mantissa);
    let (n_abs_mod, d_mod) = if scale >= 0 {
        // denominator = 10^scale
        (m_mod, powmod(10, u64::try_from(scale).unwrap_or(0), HASH_MODULUS))
    } else {
        // numerator = mantissa * 10^(-scale), denominator = 1
        (
            mulmod(
                m_mod,
                powmod(10, u64::try_from(-scale).unwrap_or(0), HASH_MODULUS),
                HASH_MODULUS,
            ),
            1,
        )
    };
    Ok(rational_hash(n_abs_mod, d_mod, mantissa.is_negative()))
}

fn fraction_hash_slot(value: &Value) -> Result<i64, EvalError> {
    use num_traits::Signed as _;
    let Value::Fraction(fr) = value else { unreachable!("fraction_hash_slot sees only Fraction") };
    // BigRational is already in lowest terms with a positive denominator.
    let n_abs_mod = bigint_abs_mod(fr.numer());
    let d_mod = bigint_abs_mod(fr.denom());
    Ok(rational_hash(n_abs_mod, d_mod, fr.numer().is_negative()))
}

/// CPython's numeric hash for a `Decimal`/`Fraction` (`None` for other types),
/// for `pyhash::python_hash` so a Decimal/Fraction can key a set/dict table and
/// share a hash with the equal int/float/rational (the slots are infallible).
#[must_use]
pub(crate) fn rational_number_hash(value: &Value) -> Option<i64> {
    match value {
        Value::Decimal(..) => decimal_hash_slot(value).ok(),
        Value::Fraction(_) => fraction_hash_slot(value).ok(),
        _ => None,
    }
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
fn fallback_hash_slot(value: &Value) -> Result<i64, EvalError> {
    use std::hash::{Hash as _, Hasher as _};
    // str/bytes/tuple/frozenset/temporals hash bit-for-bit like CPython (under
    // PYTHONHASHSEED=0) via `python_hash`, so `hash("x")`, `hash((1, 2))`, etc.
    // match the reference interpreter. It already applies CPython's `-1 → -2`
    // finalization, so return its result directly (no `finalize_hash`).
    if let Some(h) = crate::pyhash::python_hash(value) {
        return Ok(h);
    }
    // `python_hash` returns None only for a container holding an element it does
    // not cover (e.g. `(SomeInstance,)`); fall back to the structural key hash.
    // A container whose TypeObject says it is hashable (a tuple) may still hold
    // an unhashable element — `hash((1, [2]))`. `value_to_key` recurses and
    // raises `TypeError: unhashable type` on that inner element; propagate it
    // rather than returning 0 (which also collapsed every such tuple to one
    // bucket).
    let key = crate::eval::literals::value_to_key(value)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    Ok(finalize_hash(hasher.finish() as i64))
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
    let b = bytes_view(container).unwrap_or_default();
    // Shared by `bytes` and `bytearray`; the index-type error names the actual
    // container ("bytearray indices ..." vs "byte indices ...").
    let name = if matches!(container, Value::ByteArray(_)) { "bytearray" } else { "bytes" };
    let raw = int_index(index, name)?;
    let idx = normalize_seq_index(raw, b.len(), "bytes")?;
    Ok(Value::Int(i64::from(b[idx])))
}

/// `dict[key]`: hash-keyed lookup. On miss, consults the type's
/// `missing_slot` (Counter sets it; plain dict leaves it None) before
/// raising `KeyError`.
fn dict_get_item(container: &Value, index: &Value) -> Result<Value, EvalError> {
    let Some(map) = container.as_dict() else {
        unreachable!("dict_get_item only on dict/OrderedDict types")
    };
    let key = crate::eval::literals::value_to_key(index)?;
    if let Some(value) = map.lock().get(&key).cloned() {
        return Ok(value);
    }
    if let Some(missing) = type_of(container).missing_slot {
        return missing(container, index);
    }
    Err(crate::value::ExceptionValue::key_error(&key).into())
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
    let Some(map) = container.as_dict() else {
        unreachable!("dict_set_item only on dict/OrderedDict types")
    };
    let key = crate::eval::literals::value_to_key(index)?;
    let new_size = crate::state::estimate_value_size(&value);
    let delta = map.lock().insert(key.clone(), value).map_or_else(
        || to_isize_sat(crate::state::estimate_key_size(&key) + new_size),
        |old| size_delta(crate::state::estimate_value_size(&old), new_size),
    );
    Ok(delta)
}

/// `del list[i]`: remove the element at index `i`, shifting tail down.
/// Returns the (negative) byte delta.
fn list_del_item(container: &mut Value, index: &Value) -> Result<isize, EvalError> {
    let Value::List(items) = container else { unreachable!("list_del_item only on LIST_TYPE") };
    // `del lst[slice(...)]` — a computed slice object deletes the strided range,
    // matching the `del lst[i:j:k]` syntax path.
    if let Value::Slice(s) = index {
        let step = match &s.step {
            Value::None => 1,
            Value::Int(n) => *n,
            Value::Bool(b) => i64::from(*b),
            _ => {
                return Err(InterpreterError::TypeError(
                    "slice indices must be integers or None or have an __index__ method"
                        .to_string(),
                )
                .into());
            }
        };
        if step == 0 {
            return Err(InterpreterError::ValueError("slice step cannot be zero".into()).into());
        }
        let mut guard = items.lock();
        let len = i64::try_from(guard.len()).unwrap_or(i64::MAX);
        // `strided_indices` returns positions in descending order, so removing
        // them left-to-right does not shift the not-yet-removed ones.
        let indices =
            crate::eval::delete::strided_indices(Some(&s.start), Some(&s.stop), step, len);
        let mut freed = 0usize;
        for &u in &indices {
            if u < guard.len() {
                freed += crate::state::estimate_value_size(&guard[u]);
                guard.remove(u);
            }
        }
        drop(guard);
        return Ok(-to_isize_sat(freed));
    }
    let raw = int_index(index, "list")?;
    let mut guard = items.lock();
    let idx = normalize_seq_index(raw, guard.len(), "list")?;
    let removed = guard.remove(idx);
    drop(guard);
    Ok(-to_isize_sat(crate::state::estimate_value_size(&removed)))
}

/// `del dict[key]`: hash-keyed remove. Raises `KeyError` on miss.
fn dict_del_item(container: &mut Value, index: &Value) -> Result<isize, EvalError> {
    let Some(map) = container.as_dict() else {
        unreachable!("dict_del_item only on dict/OrderedDict types")
    };
    let key = crate::eval::literals::value_to_key(index)?;
    // shift_remove preserves insertion order (CPython `del d[k]`), unlike
    // swap_remove which moves the last entry into the hole.
    let Some(val) = map.lock().shift_remove(&key) else {
        return Err(crate::value::ExceptionValue::key_error(&key).into());
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
        // An IntEnum / IntFlag member has int's `__index__`, so it indexes as its
        // underlying int (`seq[P.LOW]`). A plain Enum / Flag has no `__index__`
        // and keeps the TypeError.
        Value::EnumMember {
            value,
            kind: crate::value::EnumKind::Int | crate::value::EnumKind::IntFlag,
            ..
        } => int_index(value, container_name),
        other => {
            let ty = other.type_name();
            // CPython's wording is container-specific: `str` quotes the type and
            // omits "or slices"; `bytes` reports itself as "byte"; every other
            // sequence says "integers or slices, not <type>" (unquoted).
            let msg = match container_name {
                "string" => format!("string indices must be integers, not '{ty}'"),
                "bytes" => format!("byte indices must be integers or slices, not {ty}"),
                _ => format!("{container_name} indices must be integers or slices, not {ty}"),
            };
            Err(InterpreterError::TypeError(msg).into())
        }
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
    if let Some(n) = value.set_len() {
        return Ok(n);
    }
    let Value::Tuple(items) = value else {
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

/// A single byte value from an assignment RHS: an int in `range(0, 256)`.
fn byte_from_value(value: &Value) -> Result<u8, EvalError> {
    match value {
        Value::Int(n) if (0..=255).contains(n) => Ok(*n as u8),
        Value::Bool(b) => Ok(u8::from(*b)),
        Value::Int(_) => {
            Err(InterpreterError::ValueError("byte must be in range(0, 256)".into()).into())
        }
        other => Err(InterpreterError::TypeError(format!(
            "'{}' object cannot be interpreted as an integer",
            other.type_name()
        ))
        .into()),
    }
}

/// `bytearray[i] = int` — assign a single byte in place.
fn bytearray_set_item(
    container: &mut Value,
    index: &Value,
    value: Value,
) -> Result<isize, EvalError> {
    let Value::ByteArray(ba) = container else {
        unreachable!("bytearray_set_item only on BYTEARRAY_TYPE")
    };
    let byte = byte_from_value(&value)?;
    let mut b = ba.lock();
    let raw = int_index(index, "bytearray")?;
    let idx = normalize_seq_index(raw, b.len(), "bytearray")?;
    b[idx] = byte;
    Ok(0)
}

/// `del bytearray[i]` — remove a single byte in place.
fn bytearray_del_item(container: &mut Value, index: &Value) -> Result<isize, EvalError> {
    let Value::ByteArray(ba) = container else {
        unreachable!("bytearray_del_item only on BYTEARRAY_TYPE")
    };
    let mut b = ba.lock();
    let raw = int_index(index, "bytearray")?;
    let idx = normalize_seq_index(raw, b.len(), "bytearray")?;
    b.remove(idx);
    Ok(-1)
}

fn bytearray_get_attr(value: &Value, name: &str) -> EvalResult {
    if BYTEARRAY_METHODS.contains(&name) {
        return Ok(bound_method(value, name));
    }
    Err(attribute_error("bytearray", name))
}

/// Every non-mutating method `dispatch_bytes_method` accepts, so `b.upper`,
/// `hasattr(b, "isdigit")`, and `map(bytes.upper, …)` bind as first-class
/// methods (the mutating names live only on `BYTEARRAY_METHODS`).
const BYTES_METHODS: &[&str] = &[
    "decode",
    "hex",
    "startswith",
    "endswith",
    "split",
    "rsplit",
    "replace",
    "find",
    "rfind",
    "index",
    "rindex",
    "count",
    "upper",
    "lower",
    "swapcase",
    "capitalize",
    "title",
    "isdigit",
    "isalpha",
    "isalnum",
    "isspace",
    "isupper",
    "islower",
    "istitle",
    "isascii",
    "strip",
    "lstrip",
    "rstrip",
    "join",
    "removeprefix",
    "removesuffix",
    "translate",
    "partition",
    "rpartition",
    "center",
    "ljust",
    "rjust",
    "zfill",
    "splitlines",
    "expandtabs",
    "fromhex",
    "maketrans",
];

fn bytes_get_attr(value: &Value, name: &str) -> EvalResult {
    if BYTES_METHODS.contains(&name) {
        return Ok(bound_method(value, name));
    }
    Err(attribute_error("bytes", name))
}

const MEMORYVIEW_METHODS: &[&str] = &["tobytes", "tolist", "hex"];

/// Callable (non-attribute) methods of `int`/`bool`, so `(5).bit_length` and
/// `hasattr(5, "to_bytes")` bind like CPython. The value attributes
/// (`real`/`imag`/`numerator`/`denominator`) are handled separately.
const INT_METHODS: &[&str] = &[
    "bit_length",
    "bit_count",
    "to_bytes",
    "from_bytes",
    "as_integer_ratio",
    "conjugate",
    "is_integer",
];

/// Callable methods of `float` (the `real`/`imag` value attributes aside).
const FLOAT_METHODS: &[&str] = &["is_integer", "as_integer_ratio", "hex", "fromhex", "conjugate"];

const COMPLEX_METHODS: &[&str] = &["conjugate"];

const RANGE_METHODS: &[&str] = &["count", "index"];

fn memoryview_get_attr(value: &Value, name: &str) -> EvalResult {
    if MEMORYVIEW_METHODS.contains(&name) {
        return Ok(bound_method(value, name));
    }
    // Data attributes of a 1-D unsigned-byte view (the only shape we model).
    let len = bytes_view(value).map_or(0, |b| b.len());
    // A view over immutable `bytes` is read-only; over `bytearray` it is not.
    let readonly = matches!(value, Value::MemoryView(inner) if matches!(**inner, Value::Bytes(_)));
    match name {
        "nbytes" => Ok(Value::Int(i64::try_from(len).unwrap_or(i64::MAX))),
        "itemsize" | "ndim" => Ok(Value::Int(1)),
        "format" => Ok(Value::String("B".into())),
        "shape" => Ok(Value::Tuple(vec![Value::Int(i64::try_from(len).unwrap_or(i64::MAX))])),
        "strides" => Ok(Value::Tuple(vec![Value::Int(1)])),
        "suboffsets" => Ok(Value::Tuple(Vec::new())),
        "readonly" => Ok(Value::Bool(readonly)),
        "contiguous" | "c_contiguous" | "f_contiguous" => Ok(Value::Bool(true)),
        "obj" => match value {
            Value::MemoryView(inner) => Ok((**inner).clone()),
            _ => Err(attribute_error("memoryview", name)),
        },
        _ => Err(attribute_error("memoryview", name)),
    }
}

#[expect(clippy::unnecessary_wraps, reason = "LenSlot protocol")]
fn bytes_len(value: &Value) -> Result<usize, EvalError> {
    let b = bytes_view(value).unwrap_or_default();
    Ok(b.len())
}

#[expect(clippy::unnecessary_wraps, reason = "LenSlot protocol")]
fn dict_len(value: &Value) -> Result<usize, EvalError> {
    let Some(map) = value.as_dict() else {
        unreachable!("dict_len only on dict/OrderedDict types")
    };
    let len = map.lock().len();
    Ok(len)
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
pub(crate) fn range_length(start: i64, stop: i64, step: i64) -> usize {
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
const DICT_METHODS: &[&str] = &[
    "keys",
    "values",
    "items",
    "get",
    "pop",
    "popitem",
    "update",
    "setdefault",
    "copy",
    "clear",
    "fromkeys",
];

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
    "splitlines",
    "isidentifier",
    "istitle",
    "isprintable",
    "isascii",
    "isdecimal",
    "isnumeric",
    "translate",
    "format_map",
    "maketrans",
    "rindex",
];

const LIST_METHODS: &[&str] = &[
    "append", "extend", "insert", "pop", "remove", "sort", "reverse", "index", "count", "copy",
    "clear",
];

const TUPLE_METHODS: &[&str] = &["count", "index"];

const BYTEARRAY_METHODS: &[&str] = &[
    // Mutating.
    "append",
    "extend",
    "insert",
    "remove",
    "pop",
    "clear",
    "reverse",
    // Non-mutating (shared surface with bytes).
    "copy",
    "decode",
    "hex",
    "upper",
    "lower",
    "swapcase",
    "capitalize",
    "title",
    "isdigit",
    "isalpha",
    "isalnum",
    "isspace",
    "isupper",
    "islower",
    "strip",
    "lstrip",
    "rstrip",
    "split",
    "replace",
    "find",
    "rfind",
    "index",
    "rindex",
    "count",
    "startswith",
    "endswith",
    "removeprefix",
    "removesuffix",
    "join",
    "isascii",
    "istitle",
    "expandtabs",
    "rsplit",
    "translate",
    "partition",
    "rpartition",
    "center",
    "ljust",
    "rjust",
    "zfill",
    "splitlines",
    "fromhex",
    "maketrans",
];

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

/// `int.real`/`.numerator` (the int itself), `.imag` (`0`), `.denominator` (`1`)
/// — the numeric-tower attributes CPython exposes on `int`.
fn int_get_attr(value: &Value, name: &str) -> EvalResult {
    match name {
        "real" | "numerator" => Ok(value.clone()),
        "imag" => Ok(Value::Int(0)),
        "denominator" => Ok(Value::Int(1)),
        _ if INT_METHODS.contains(&name) => Ok(bound_method(value, name)),
        _ => Err(attribute_error("int", name)),
    }
}

/// `bool` shares int's numeric-tower attributes, but yields plain ints
/// (`True.real == 1`, not `True`).
fn bool_get_attr(value: &Value, name: &str) -> EvalResult {
    let Value::Bool(b) = value else { unreachable!("bool_get_attr sees only Bool") };
    let n = i64::from(*b);
    match name {
        "real" | "numerator" => Ok(Value::Int(n)),
        "imag" => Ok(Value::Int(0)),
        "denominator" => Ok(Value::Int(1)),
        // bool is an int subclass, so it exposes int's methods too.
        _ if INT_METHODS.contains(&name) => Ok(bound_method(value, name)),
        _ => Err(attribute_error("bool", name)),
    }
}

/// `float.real` (itself) / `.imag` (`0.0`), plus float's callable methods.
fn float_get_attr(value: &Value, name: &str) -> EvalResult {
    match name {
        "real" => Ok(value.clone()),
        "imag" => Ok(Value::Float(0.0)),
        _ if FLOAT_METHODS.contains(&name) => Ok(bound_method(value, name)),
        _ => Err(attribute_error("float", name)),
    }
}

/// `complex.real` / `complex.imag` attribute access (both `float`). `.conjugate`
/// is a method, dispatched through the method table.
fn complex_get_attr(value: &Value, name: &str) -> EvalResult {
    let Value::Complex(c) = value else { unreachable!("complex_get_attr sees only Complex") };
    match name {
        "real" => Ok(Value::Float(c.re)),
        "imag" => Ok(Value::Float(c.im)),
        _ if COMPLEX_METHODS.contains(&name) => Ok(bound_method(value, name)),
        _ => Err(attribute_error("complex", name)),
    }
}

/// `range.start`/`.stop`/`.step` value attributes plus its `count`/`index`
/// methods (CPython exposes all five).
fn range_get_attr(value: &Value, name: &str) -> EvalResult {
    let Value::Range { start, stop, step } = value else {
        unreachable!("range_get_attr sees only Range")
    };
    match name {
        "start" => Ok(Value::Int(*start)),
        "stop" => Ok(Value::Int(*stop)),
        "step" => Ok(Value::Int(*step)),
        _ if RANGE_METHODS.contains(&name) => Ok(bound_method(value, name)),
        _ => Err(attribute_error("range", name)),
    }
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

/// `frozenset` exposes only the non-mutating set methods; the mutators
/// (`add`, `update`, `pop`, …) are absent, so `fs.add` raises AttributeError
/// exactly as CPython's immutable frozenset does.
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

fn frozenset_get_attr(value: &Value, name: &str) -> EvalResult {
    if FROZENSET_METHODS.contains(&name) {
        return Ok(bound_method(value, name));
    }
    Err(attribute_error("frozenset", name))
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
    // Compare against the other map's contents (Counter stores an
    // IndexMap by value; Dict is behind a lock).
    let compare = |b: &indexmap::IndexMap<crate::value::ValueKey, Value>| {
        a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|bv| recurse_eq(v, bv)))
    };
    match rhs {
        Value::Counter(b) => Some(compare(b)),
        Value::Dict(b) => Some(compare(&b.lock())),
        _ => None,
    }
}

/// `key in counter`: same hash-keyed lookup as dict. A Counter
/// reports membership based on stored entries, not non-zero values
/// — `c["missing"] == 0` but `"missing" in c` is False (matching
/// CPython).
fn counter_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let Value::Counter(map) = container else {
        unreachable!("counter_contains only on COUNTER_TYPE")
    };
    // Unhashable probe raises, does not answer False.
    let key = crate::eval::literals::value_to_key(item)?;
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
    Err(crate::value::ExceptionValue::key_error(&key).into())
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
    // shift_remove preserves insertion order (CPython `del d[k]`), unlike
    // swap_remove which moves the last entry into the hole.
    let Some(val) = map.shift_remove(&key) else {
        return Err(crate::value::ExceptionValue::key_error(&key).into());
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
/// involved than needed in our eager-extract workload. Deque element-wise
/// equality is tracked by `gap-deque-equality-parity`.
const fn noimpl_eq(_lhs: &Value, _rhs: &Value) -> Option<bool> {
    None
}

/// `deque == deque` — element-wise in order (a deque never equals a list).
fn deque_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let (Value::Deque { items: a, .. }, Value::Deque { items: b, .. }) = (lhs, rhs) else {
        return None;
    };
    Some(
        a.len() == b.len()
            && a.iter().zip(b.iter()).all(|(x, y)| crate::eval::operations::values_equal_pub(x, y)),
    )
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

fn deque_set_item(container: &mut Value, index: &Value, value: Value) -> Result<isize, EvalError> {
    let Value::Deque { items, .. } = container else {
        unreachable!("deque_set_item only on DEQUE_TYPE")
    };
    let raw = int_index(index, "deque")?;
    let idx = normalize_seq_index(raw, items.len(), "deque")?;
    let new_size = crate::state::estimate_value_size(&value);
    let old = std::mem::replace(&mut items[idx], value);
    Ok(crate::eval::place::size_delta(crate::state::estimate_value_size(&old), new_size))
}

fn deque_del_item(container: &mut Value, index: &Value) -> Result<isize, EvalError> {
    let Value::Deque { items, .. } = container else {
        unreachable!("deque_del_item only on DEQUE_TYPE")
    };
    let raw = int_index(index, "deque")?;
    let idx = normalize_seq_index(raw, items.len(), "deque")?;
    let removed = items.remove(idx);
    Ok(-crate::eval::place::to_isize(removed.as_ref().map_or(0, crate::state::estimate_value_size)))
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
        "index",
        "count",
        "insert",
        "remove",
        "reverse",
    ];
    if DEQUE_METHODS.contains(&name) {
        return Ok(bound_method(value, name));
    }
    // `.maxlen` is the bound (or None) capacity, read-only in CPython.
    if name == "maxlen" {
        let Value::Deque { maxlen, .. } = value else {
            unreachable!("deque_get_attr only on DEQUE_TYPE")
        };
        return Ok(maxlen.map_or(Value::None, |n| Value::Int(i64::try_from(n).unwrap_or(i64::MAX))));
    }
    Err(attribute_error("deque", name))
}

/// `key in defaultdict` — same as dict.
fn defaultdict_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let Value::DefaultDict(data) = container else {
        unreachable!("defaultdict_contains only on DEFAULTDICT_TYPE")
    };
    // Unhashable probe raises, does not answer False.
    let key = crate::eval::literals::value_to_key(item)?;
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
    // shift_remove preserves insertion order (CPython `del dd[k]`).
    let Some(val) = data.items.shift_remove(&key) else {
        return Err(crate::value::ExceptionValue::key_error(&key).into());
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
        Value::Decimal(d, _) => Some((**d).clone()),
        Value::Int(i) => Some(bigdecimal::BigDecimal::from(*i)),
        Value::BigInt(i) => Some(bigdecimal::BigDecimal::from(i.as_ref().clone())),
        Value::Bool(b) => Some(bigdecimal::BigDecimal::from(i64::from(*b))),
        _ => None,
    }
}

fn decimal_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    use crate::value::DecimalKind as K;
    // Infinity / NaN: NaN never equals anything (even itself); infinities are
    // equal only to a same-signed infinity.
    let (ka, kb) = (decimal_operand_kind(lhs), decimal_operand_kind(rhs));
    if ka.is_special() || kb.is_special() {
        if ka.is_nan() || kb.is_nan() {
            return Some(false);
        }
        return Some(matches!((ka, kb), (K::PosInf, K::PosInf) | (K::NegInf, K::NegInf)));
    }
    // `Decimal == float` / `Decimal == Fraction` is a legal comparison in
    // CPython (unlike `Decimal + float`, which raises) and is exact: both sides
    // reduce to a rational and compare mathematically. The int/Decimal path
    // stays on `BigDecimal` to avoid a `10^exponent` blow-up on a hostile scale.
    if matches!(rhs, Value::Float(_) | Value::Fraction(_)) {
        return Some(decimal_to_bigrational(lhs)? == exact_rational(rhs)?);
    }
    Some(decimal_to_bigdecimal(lhs)? == decimal_to_bigdecimal(rhs)?)
}

fn decimal_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    // Infinity / NaN ordering: a NaN comparison raises InvalidOperation
    // (CPython), while `-Infinity < finite < +Infinity`.
    let (ka, kb) = (decimal_operand_kind(lhs), decimal_operand_kind(rhs));
    if ka.is_special() || kb.is_special() {
        if ka.is_nan() || kb.is_nan() {
            return Some(Err(EvalError::Exception(crate::value::ExceptionValue::new(
                "InvalidOperation",
                "comparison involving NaN",
            ))));
        }
        let rank = |k: crate::value::DecimalKind| match k {
            crate::value::DecimalKind::NegInf => -2_i32,
            crate::value::DecimalKind::PosInf => 2,
            _ => 0,
        };
        return Some(Ok(rank(ka) < rank(kb)));
    }
    // `Decimal` vs `float`/`Fraction` (either operand can be the Decimal, since
    // the dispatcher tries this slot for both positions) compares by exact
    // value, matching CPython (`Decimal(3) < 3.5` is legal, unlike arithmetic).
    let mixed = (matches!(lhs, Value::Decimal(..))
        && matches!(rhs, Value::Float(_) | Value::Fraction(..)))
        || (matches!(rhs, Value::Decimal(..))
            && matches!(lhs, Value::Float(_) | Value::Fraction(..)));
    if mixed {
        return Some(Ok(tower_partial_cmp(lhs, rhs) == Some(std::cmp::Ordering::Less)));
    }
    Some(Ok(decimal_to_bigdecimal(lhs)? < decimal_to_bigdecimal(rhs)?))
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
    // Infinity / NaN operands follow IEEE-style rules, not BigDecimal math (the
    // BigDecimal is a placeholder for those).
    if decimal_operand_kind(lhs).is_special() || decimal_operand_kind(rhs).is_special() {
        return Some(decimal_special_arith(op, lhs, rhs));
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
            // BigDecimal lacks a direct floor-div; Decimal `//` truncates the
            // quotient toward zero (unlike int floor division).
            (a / b).with_scale(0)
        }
        BinOp::Mod => {
            if b.is_zero() {
                return Some(Err(
                    InterpreterError::Runtime("Decimal division by zero".into()).into()
                ));
            }
            // CPython Decimal remainder: `a - (a // b) * b`, where `//`
            // truncates toward zero, so the remainder takes the sign of `a`.
            let q = (a.clone() / b.clone()).with_scale(0);
            a - q * b
        }
        BinOp::Pow => {
            use num_traits::ToPrimitive as _;
            // Integer exponent -> exact repeated multiplication; a negative
            // exponent inverts and rounds to the context precision. A
            // fractional exponent is not modelled.
            let exp = (b.fractional_digit_count() <= 0).then(|| b.to_i64()).flatten()?;
            let mut acc = bigdecimal::BigDecimal::from(1);
            for _ in 0..exp.unsigned_abs() {
                acc *= &a;
            }
            if exp < 0 {
                if a.is_zero() {
                    return Some(Err(
                        InterpreterError::Runtime("Decimal division by zero".into()).into()
                    ));
                }
                let digits = u64::try_from(decimal_prec).unwrap_or(28);
                let inv = bigdecimal::BigDecimal::from(1) / acc;
                if inv.digits() > digits { inv.with_prec(digits) } else { inv }
            } else {
                acc
            }
        }
    };
    Some(Ok(Value::Decimal(Box::new(result), crate::value::DecimalKind::Normal)))
}

/// The [`DecimalKind`] of an arithmetic operand: the tag for a `Decimal`, or
/// `Normal` for an int/bool that lifts into the operation.
fn decimal_operand_kind(v: &Value) -> crate::value::DecimalKind {
    match v {
        Value::Decimal(_, k) => *k,
        _ => crate::value::DecimalKind::Normal,
    }
}

/// Whether an operand's value is negative (for combining signs with an infinite
/// operand). A finite operand reads its sign from the number; `NegZero` counts
/// as negative for sign propagation.
fn decimal_operand_negative(v: &Value) -> bool {
    use num_traits::Signed as _;
    match v {
        Value::Decimal(d, k) => {
            matches!(k, crate::value::DecimalKind::NegInf | crate::value::DecimalKind::NegZero)
                || d.is_negative()
        }
        Value::Int(i) => *i < 0,
        Value::BigInt(b) => b.is_negative(),
        _ => false,
    }
}

/// IEEE-754-style arithmetic when at least one operand is Infinity/NaN, matching
/// CPython's `decimal`. Returns the special result, `Ok` with a placeholder
/// `BigDecimal` and the right kind (or a `DivisionByZero`/`InvalidOperation`
/// error where CPython raises).
fn decimal_special_arith(op: BinOp, lhs: &Value, rhs: &Value) -> Result<Value, EvalError> {
    use crate::value::DecimalKind as K;
    use num_traits::Zero as _;
    let mk = |k: K| Ok(Value::Decimal(Box::new(bigdecimal::BigDecimal::from(0)), k));
    // CPython's default context traps `InvalidOperation`: an op that would
    // CREATE a NaN from non-NaN operands (inf-inf, inf*0, inf/inf) raises,
    // whereas a NaN OPERAND merely propagates.
    let invalid = || {
        Err(EvalError::Exception(crate::value::ExceptionValue::new(
            "InvalidOperation",
            "[<class 'decimal.InvalidOperation'>]",
        )))
    };
    let (ka, kb) = (decimal_operand_kind(lhs), decimal_operand_kind(rhs));
    let (na, nb) = (decimal_operand_negative(lhs), decimal_operand_negative(rhs));
    // A NaN operand propagates through every arithmetic op (no trap).
    if ka.is_nan() || kb.is_nan() {
        return mk(K::Nan);
    }
    let inf = |neg: bool| if neg { K::NegInf } else { K::PosInf };
    let a_zero = matches!(lhs, Value::Decimal(d, k) if !k.is_special() && d.is_zero());
    let b_zero = matches!(rhs, Value::Decimal(d, k) if !k.is_special() && d.is_zero());
    match op {
        BinOp::Add => match (ka.is_infinite(), kb.is_infinite()) {
            (true, true) => {
                if na == nb {
                    mk(inf(na))
                } else {
                    invalid() // inf + -inf
                }
            }
            (true, false) => mk(inf(na)),
            (false, true) => mk(inf(nb)),
            (false, false) => mk(K::Normal),
        },
        BinOp::Sub => match (ka.is_infinite(), kb.is_infinite()) {
            (true, true) => {
                if na != nb {
                    mk(inf(na))
                } else {
                    invalid() // inf - inf
                }
            }
            (true, false) => mk(inf(na)),
            (false, true) => mk(inf(!nb)),
            (false, false) => mk(K::Normal),
        },
        BinOp::Mul => {
            if (ka.is_infinite() && b_zero) || (kb.is_infinite() && a_zero) {
                return invalid(); // inf * 0
            }
            if ka.is_infinite() || kb.is_infinite() {
                return mk(inf(na != nb));
            }
            mk(K::Normal)
        }
        BinOp::Div => match (ka.is_infinite(), kb.is_infinite()) {
            (true, true) => invalid(),          // inf / inf
            (true, false) => mk(inf(na != nb)), // inf / finite
            (false, true) => {
                // finite / inf -> a signed zero pinned to the context's Etiny
                // exponent (Emin - prec + 1 = -999999 - 28 + 1 = -1000026 for
                // the default context), so `D(1)/D('inf')` reprs `0E-1000026`.
                let zero = bigdecimal::BigDecimal::new(num_bigint::BigInt::from(0), 1_000_026);
                let sign = if na != nb { K::NegZero } else { K::Normal };
                Ok(Value::Decimal(Box::new(zero), sign))
            }
            (false, false) => mk(K::Normal),
        },
        // FloorDiv/Mod/Pow with an infinity trap InvalidOperation in CPython too.
        _ => invalid(),
    }
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

/// The exact value of `f64` as a rational (via its IEEE mantissa/exponent), so
/// `Fraction`/`Decimal` compare to a float without precision loss. `None` for a
/// non-finite float, which equals no rational.
fn float_to_bigrational(f: f64) -> Option<num_rational::BigRational> {
    use num_bigint::BigInt;
    use num_traits::Float as _;
    if !f.is_finite() {
        return None;
    }
    // f == sign * mantissa * 2^exp, exactly.
    let (mantissa, exp, sign) = f.integer_decode();
    let numer = BigInt::from(mantissa) * BigInt::from(i64::from(sign));
    if exp >= 0 {
        Some(num_rational::BigRational::from_integer(numer << usize::try_from(exp).ok()?))
    } else {
        Some(num_rational::BigRational::new(numer, BigInt::from(1) << usize::try_from(-exp).ok()?))
    }
}

/// A `Decimal` as an exact rational: `mantissa * 10^(-scale)`. `None` if the
/// scale is too large to materialise (such a value equals no finite float).
fn decimal_to_bigrational(value: &Value) -> Option<num_rational::BigRational> {
    use num_bigint::BigInt;
    let Value::Decimal(d, _) = value else { return None };
    let (mantissa, scale) = d.as_bigint_and_exponent();
    let ten = BigInt::from(10);
    if scale >= 0 {
        Some(num_rational::BigRational::new(mantissa, ten.pow(u32::try_from(scale).ok()?)))
    } else {
        Some(num_rational::BigRational::from_integer(
            mantissa * ten.pow(u32::try_from(-scale).ok()?),
        ))
    }
}

/// Any exact-numeric `Value` (including a float via its exact rational) as a
/// `BigRational`; `None` for non-numerics and non-finite floats.
fn exact_rational(value: &Value) -> Option<num_rational::BigRational> {
    match value {
        Value::Float(f) => float_to_bigrational(*f),
        Value::Decimal(..) => decimal_to_bigrational(value),
        _ => fraction_to_bigrational(value),
    }
}

/// Order two numeric-tower values by exact value, handling a non-finite float
/// operand: a NaN yields `None` (every ordering with NaN is false), and ±inf
/// ranks above/below every finite value. `None` if either operand is not a
/// tower number. Used by the Decimal/Fraction `lt` slots for mixed comparisons,
/// where either operand may be the Decimal/Fraction.
fn tower_partial_cmp(lhs: &Value, rhs: &Value) -> Option<std::cmp::Ordering> {
    // NaN is unordered against everything.
    if matches!(lhs, Value::Float(f) if f.is_nan()) || matches!(rhs, Value::Float(f) if f.is_nan())
    {
        return None;
    }
    // Rank ±inf outside the finite rationals; a finite value ranks 0 and is
    // then compared exactly.
    let rank = |v: &Value| match v {
        Value::Float(f) if f.is_infinite() => {
            if *f > 0.0 {
                1
            } else {
                -1
            }
        }
        _ => 0,
    };
    match (rank(lhs), rank(rhs)) {
        (0, 0) => Some(exact_rational(lhs)?.cmp(&exact_rational(rhs)?)),
        (a, b) => Some(a.cmp(&b)),
    }
}

fn fraction_eq(lhs: &Value, rhs: &Value) -> Option<bool> {
    let a = fraction_to_bigrational(lhs)?;
    // `Fraction == float` / `Fraction == Decimal` compares exact rationals,
    // matching CPython (`Fraction(1, 2) == 0.5` is True, `Fraction(1, 3) ==
    // 1/3.0` is False because the float is not exactly a third).
    if matches!(rhs, Value::Float(_) | Value::Decimal(..)) {
        return Some(exact_rational(rhs).is_some_and(|b| a == b));
    }
    Some(a == fraction_to_bigrational(rhs)?)
}

fn fraction_lt(lhs: &Value, rhs: &Value) -> Option<Result<bool, EvalError>> {
    // `Fraction` vs `float`/`Decimal` (either position) compares exact values.
    let mixed = (matches!(lhs, Value::Fraction(..))
        && matches!(rhs, Value::Float(_) | Value::Decimal(..)))
        || (matches!(rhs, Value::Fraction(..))
            && matches!(lhs, Value::Float(_) | Value::Decimal(..)));
    if mixed {
        return Some(Ok(tower_partial_cmp(lhs, rhs) == Some(std::cmp::Ordering::Less)));
    }
    Some(Ok(fraction_to_bigrational(lhs)? < fraction_to_bigrational(rhs)?))
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
            // CPython: `Fraction // Fraction` yields an int, not a Fraction.
            let floored = (a / b).floor();
            return Some(Ok(crate::value::int_from_bigint(floored.to_integer())));
        }
        BinOp::Mod => {
            if b.numer().sign() == num_bigint::Sign::NoSign {
                return Some(Err(
                    InterpreterError::Runtime("Fraction division by zero".into()).into()
                ));
            }
            // `a - floor(a / b) * b` — floored remainder, matching CPython.
            let q = (a.clone() / b.clone()).floor();
            a - q * b
        }
        BinOp::Pow => {
            use num_traits::ToPrimitive as _;
            // An integer exponent is exact (`Ratio::pow` inverts for negative);
            // a non-integer exponent falls back to float via the caller.
            if !b.is_integer() {
                let base = fraction_to_f64(lhs)?;
                let exp = fraction_to_f64(rhs)?;
                return Some(Ok(Value::Float(base.powf(exp))));
            }
            let exp = b.numer().to_i32()?;
            if exp < 0 && a.numer().sign() == num_bigint::Sign::NoSign {
                return Some(Err(
                    InterpreterError::Runtime("Fraction division by zero".into()).into()
                ));
            }
            a.pow(exp)
        }
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
    // Keep the i64 fast path, but promote past i64 to an exact BigInt rather
    // than a lossy float — a Fraction numerator/denominator is an exact integer.
    crate::value::int_from_bigint(value.clone())
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
        // `timedelta / timedelta` (ratio) and `timedelta / int` both route here.
        BinOp::Div => "/",
        BinOp::Mod => "%",
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

/// `member in flag` — a Flag/IntFlag contains another flag when all the RHS's
/// bits are set (`(self.value & item.value) == item.value`). Non-flag enum
/// members are not containers, matching CPython's TypeError.
fn enummember_contains(container: &Value, item: &Value) -> Result<bool, EvalError> {
    let Value::EnumMember { kind, value: cv, .. } = container else {
        unreachable!("enummember_contains only on ENUMMEMBER_TYPE")
    };
    if !kind.is_flag() {
        return Err(
            InterpreterError::TypeError("argument of type 'enum' is not iterable".into()).into()
        );
    }
    let container_bits = crate::value::value_as_i64(cv).unwrap_or(0);
    let item_bits = match item {
        Value::EnumMember { value, .. } => crate::value::value_as_i64(value).unwrap_or(-1),
        _ => {
            return Err(InterpreterError::TypeError(format!(
                "unsupported operand type(s) for 'in': '{}' and 'enum'",
                item.type_name()
            ))
            .into());
        }
    };
    Ok(item_bits >= 0 && container_bits & item_bits == item_bits)
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
    // A user-class instance reports its own class name, not the generic
    // "object" that its static TypeObject carries.
    let type_name = |v: &Value| match v {
        Value::Instance(inst) => inst.class_name.clone(),
        other => type_of(other).name.to_string(),
    };
    InterpreterError::TypeError(format!(
        "'{op}' not supported between instances of '{}' and '{}'",
        type_name(lhs),
        type_name(rhs),
    ))
    .into()
}
