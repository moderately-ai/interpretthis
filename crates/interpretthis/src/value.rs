// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{collections::BTreeMap, fmt, sync::Arc};

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use compact_str::CompactString;
use indexmap::IndexMap;
use parking_lot::Mutex;

/// Shared, interior-mutable list storage backing `Value::List`.
///
/// Cloning a `Value::List` is a refcount bump on the same `Arc`, and
/// mutation via any alias is visible through every other alias —
/// matching CPython's reference semantics for chained assignment,
/// mutable defaults, and closure captures.
///
/// `Mutex` (not `RefCell`) so `Value` stays `Send` across `.await` points
/// inside `Interpreter::execute`. Hot loops pay lock overhead; a
/// single-thread `RefCell` model is not used because tool futures and
/// async eval interleave on the runtime.
pub type SharedList = Arc<Mutex<Vec<Value>>>;

/// Shared instance-field map backing [`InstanceValue::fields`].
///
/// Same identity model as [`SharedList`]: cloning an instance Value is a
/// refcount bump, so `setattr` / field writes through any alias are
/// visible on every other alias — matching CPython object identity.
pub type SharedFields = Arc<Mutex<BTreeMap<String, Value>>>;

/// Construct a fresh `SharedList` from a `Vec<Value>`. Centralised so
/// call sites use this instead of inlining `Arc::new(Mutex::new(v))`.
#[inline]
#[must_use]
pub fn shared_list(items: Vec<Value>) -> SharedList {
    Arc::new(Mutex::new(items))
}

/// Mutable, shared backing store for [`Value::ByteArray`]. Same identity model
/// as [`SharedList`]: a `bytearray` clone shares storage, so in-place mutation
/// (`b[0] = ...`, `.append(...)`) is visible through every alias.
pub type SharedByteArray = Arc<Mutex<Vec<u8>>>;

/// Construct a fresh `SharedByteArray` from a `Vec<u8>`.
#[inline]
#[must_use]
pub fn shared_bytes(bytes: Vec<u8>) -> SharedByteArray {
    Arc::new(Mutex::new(bytes))
}

/// Construct a fresh [`SharedFields`] map.
#[inline]
#[must_use]
pub fn shared_fields(fields: BTreeMap<String, Value>) -> SharedFields {
    Arc::new(Mutex::new(fields))
}

// ---------------------------------------------------------------------------
// Python int policy (hybrid i64 + BigInt)
//
// - Arithmetic promotes via [`int_from_bigint`] so results that fit i64 stay
//   on the fast path.
// - Indices, lengths, and other size-like uses go through [`value_as_i64`]
//   / OverflowError when out of range (see operations::to_int).
// - Pure-arithmetic paths prefer [`value_as_bigint`].
// ---------------------------------------------------------------------------

/// Build a Python int value, using [`Value::Int`] when it fits in i64.
#[inline]
#[must_use]
pub fn int_from_bigint(n: num_bigint::BigInt) -> Value {
    match i64::try_from(&n) {
        Ok(v) => Value::Int(v),
        Err(_) => Value::BigInt(Box::new(n)),
    }
}

/// Like [`int_from_bigint`] but rejects magnitudes beyond `max_bits`.
pub(crate) fn int_from_bigint_limited(
    n: num_bigint::BigInt,
    max_bits: u64,
) -> Result<Value, crate::error::EvalError> {
    use crate::error::EvalError;
    use crate::value::ExceptionValue;
    // bits() is magnitude bits; allow a little headroom for sign.
    if n.bits() > max_bits {
        return Err(EvalError::Exception(ExceptionValue::new(
            "OverflowError",
            format!("int exceeds max_int_bits limit ({max_bits} bits)"),
        )));
    }
    Ok(int_from_bigint(n))
}

/// Lift a Python numeric value to `BigInt` (int / bigint / bool).
#[must_use]
pub fn value_as_bigint(v: &Value) -> Option<num_bigint::BigInt> {
    match v {
        Value::Int(i) => Some(num_bigint::BigInt::from(*i)),
        Value::BigInt(b) => Some((**b).clone()),
        Value::Bool(b) => Some(num_bigint::BigInt::from(i64::from(*b))),
        _ => None,
    }
}

/// Narrow to i64 when the value is a small int (or bool).
#[must_use]
pub fn value_as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Int(i) => Some(*i),
        Value::Bool(b) => Some(i64::from(*b)),
        Value::BigInt(b) => i64::try_from(b.as_ref()).ok(),
        _ => None,
    }
}

/// Serialize a `SharedList` as if it were `Vec<Value>` (locking the
/// inner mutex for the duration of the seq emission). The wire format
/// matches what the old un-shared `Value::List(Vec<Value>)` produced,
/// so serialized state from before D2 deserializes cleanly into the
/// new shape.
fn serialize_shared_list<S: serde::Serializer>(
    list: &SharedList,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let snapshot = list.lock().clone();
    let mut seq = serializer.serialize_seq(Some(snapshot.len()))?;
    for v in &snapshot {
        seq.serialize_element(v)?;
    }
    seq.end()
}

/// Deserialize a list of values into a `SharedList`. Reads the same
/// wire format as `Vec<Value>` produces and wraps in a fresh `Arc<Mutex>`.
fn deserialize_shared_list<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<SharedList, D::Error> {
    let items: Vec<Value> = Deserialize::deserialize(deserializer)?;
    Ok(shared_list(items))
}

fn serialize_shared_bytes<S: serde::Serializer>(
    bytes: &SharedByteArray,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(&bytes.lock())
}

fn deserialize_shared_bytes<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<SharedByteArray, D::Error> {
    let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
    Ok(shared_bytes(bytes))
}

/// Serialize instance fields as a plain map (lock, then emit).
fn serialize_shared_fields<S: serde::Serializer>(
    fields: &SharedFields,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let snapshot = fields.lock().clone();
    snapshot.serialize(serializer)
}

/// Deserialize a map into [`SharedFields`].
fn deserialize_shared_fields<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<SharedFields, D::Error> {
    let map: BTreeMap<String, Value> = Deserialize::deserialize(deserializer)?;
    Ok(shared_fields(map))
}
// `Zero` provides `is_zero` on `BigDecimal`/`BigRational`/`BigInt` —
// used by `is_truthy` for the `Decimal` and `Fraction` variants.
use num_traits::Zero as _;
use serde::{Deserialize, Serialize};

/// Serialize an `IndexMap`<`ValueKey`, Value> as a list of [key, value] pairs.
fn serialize_dict<S: serde::Serializer>(
    map: &IndexMap<ValueKey, Value>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut seq = serializer.serialize_seq(Some(map.len()))?;
    for (k, v) in map {
        seq.serialize_element(&(k, v))?;
    }
    seq.end()
}

/// Deserialize a list of [key, value] pairs into an `IndexMap`<`ValueKey`, Value>.
fn deserialize_dict<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<IndexMap<ValueKey, Value>, D::Error> {
    let pairs: Vec<(ValueKey, Value)> = Deserialize::deserialize(deserializer)?;
    Ok(pairs.into_iter().collect())
}

/// The dynamic value type flowing through the interpreter.
///
/// Which builtin lazy iterator a [`Value::BuiltinIter`] is, carried
/// inline so `type()`/`repr` need no state lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuiltinIterName {
    Count,
    Cycle,
    Repeat,
}

impl BuiltinIterName {
    /// The CPython type name (`type(itertools.count()).__name__`).
    #[must_use]
    pub const fn type_name(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Cycle => "cycle",
            Self::Repeat => "repeat",
        }
    }
}

/// Every variable, tool argument, and return value is a `Value`. This enum
/// covers all Python types the interpreter supports. Use the `as_*()` methods
/// for borrowing access, `try_into_*()` for consuming access, or pattern matching.
///
/// Implements `PartialEq` (floats compared bitwise, `LazyProxy` is never equal).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Value {
    /// Python `None`.
    None,
    /// Python `NotImplemented` singleton — dunder methods return this to
    /// signal "try the reflected operand / other protocol path".
    NotImplemented,
    /// Python `Ellipsis` (`...`) singleton — a distinct truthy object, not
    /// `None`.
    Ellipsis,
    /// Python `bool`.
    Bool(bool),
    /// Python `int` that fits in i64 (fast path).
    Int(i64),
    /// Python `int` outside the i64 range (arbitrary precision).
    /// Always boxed; use [`int_from_bigint`] so values that fit i64
    /// stay on the fast [`Self::Int`] path.
    BigInt(Box<num_bigint::BigInt>),
    /// Python `float` (IEEE 754 f64).
    Float(f64),
    /// Python `complex` — a pair of f64 (real, imaginary). Boxed to keep the
    /// `Value` slot small (matching [`Self::Decimal`]/[`Self::Fraction`]).
    Complex(Box<num_complex::Complex64>),
    /// Python `str`. Backed by [`CompactString`] — strings up to 24 B
    /// of UTF-8 stay inline (no heap allocation), longer strings spill
    /// to the heap with `String`'s layout. The footprint matches a
    /// plain `String` (24 B); the SSO is a pure dropped-allocation win
    /// for the typical LLM-emitted short literals (dict keys like
    /// `"id"`, `"name"`, `"category"`, row identifiers like `"row_42"`).
    String(CompactString),
    /// Python `bytes`.
    Bytes(Vec<u8>),
    /// Python `bytearray` — a mutable sequence of bytes with shared identity
    /// (see [`SharedByteArray`]). Distinct from the immutable `Bytes` so item /
    /// slice assignment, `.append`, `del`, etc. mutate in place.
    ByteArray(
        #[serde(serialize_with = "serialize_shared_bytes")]
        #[serde(deserialize_with = "deserialize_shared_bytes")]
        SharedByteArray,
    ),
    /// Python `memoryview` — a read view over a `bytes`/`bytearray` buffer. The
    /// inner value is the source (a `ByteArray` shares its storage, so the view
    /// reflects mutations; a `Bytes` source is a fixed snapshot). Boxed to keep
    /// the enum slot narrow.
    MemoryView(Box<Value>),
    /// Python `list`. Backed by `Arc<Mutex<Vec<Value>>>` so chained
    /// assignment (`a = b = []`), mutable default args, and closure
    /// captures of lists share the same identity — mutations via any
    /// alias are visible to all aliases, matching CPython's reference
    /// semantics. Cloning a `Value::List` is a refcount bump; deep
    /// copies go through `list.copy()` or `copy.copy`.
    List(
        #[serde(serialize_with = "serialize_shared_list")]
        #[serde(deserialize_with = "deserialize_shared_list")]
        SharedList,
    ),
    /// Python `tuple`.
    Tuple(Vec<Self>),
    /// Python `dict` (ordered, hashable keys only).
    /// Serialized as a list of `[key, value]` pairs since JSON requires string keys.
    Dict(
        #[serde(serialize_with = "serialize_dict", deserialize_with = "deserialize_dict")]
        IndexMap<ValueKey, Self>,
    ),
    /// Python `set` (stored as Vec since Value isn't Hash).
    Set(Vec<Self>),
    /// Python `frozenset` — an immutable, hashable set. Same Vec storage as
    /// `Set`, but with no mutating methods and a `ValueKey::Frozenset`
    /// projection so it can serve as a dict key or set member.
    Frozenset(Vec<Self>),
    /// User-defined function (`def`) — captures closure at definition time.
    ///
    /// Wrapped in `Arc` so closures that reference a function share the
    /// underlying definition (and its captured closure) by pointer rather than
    /// deep-cloning the whole tree on every binding. Without this, sequential
    /// `def`s would copy each prior function's closure into the next one,
    /// growing storage as O(2^N) in the number of definitions (see F2.5).
    Function(Arc<FunctionDef>),
    /// Lambda expression — captures free names at definition time (see
    /// [`LambdaDef::closure`]). `Arc`-wrapped for cheap clone when lambdas are
    /// passed as arguments or assigned to variables.
    Lambda(Arc<LambdaDef>),
    /// Python `range()` result.
    Range { start: i64, stop: i64, step: i64 },
    /// A `slice(start, stop, step)` object. Each bound is a `Value` (an `Int`
    /// or `None`, mirroring CPython's `slice.start`/`.stop`/`.step`), boxed to
    /// keep the enum slot narrow. Usable as a subscript index.
    Slice(Box<SliceValue>),
    /// Runtime exception instance (for `try`/`except`/`raise`).
    Exception(Box<ExceptionValue>),
    /// Bound method on an exception instance (`eg.subgroup`, `eg.split`).
    ExceptionMethod { method: String, exception: Box<ExceptionValue> },
    /// Deferred tool result — resolved lazily when consumed.
    /// Not serializable; filtered before state export.
    #[serde(skip)]
    LazyProxy(crate::tools::lazy_proxy::LazyProxy),
    /// A stateful iterator wrapping an eagerly-materialised
    /// generator. Each call to `next(g)` advances the cursor by one;
    /// iteration consumes the remaining items. Internally the cursor
    /// lives in `InterpreterState::lazy_cursors` keyed by `cursor_id`
    /// so the variant stays cheaply cloneable (the same `Value::Lazy`
    /// stored in two variables shares one cursor — matching CPython's
    /// reference semantics for generators bound to multiple names).
    Lazy { items: Vec<Self>, cursor_id: u64 },
    /// Suspended generator function (`def g(): yield ...`). Frame state
    /// lives in `InterpreterState::generators` keyed by `id`.
    Generator { id: u64 },
    /// A builtin lazy iterator with no backing AST — the potentially
    /// infinite `itertools` producers (`count`, `cycle`, unbounded
    /// `repeat`). Cursor state lives in
    /// `InterpreterState::builtin_iters` keyed by `id`, so the handle
    /// stays cheaply cloneable and two names share one cursor (CPython
    /// iterator reference semantics). `kind` is carried inline for
    /// `type()`/`repr` without a state lookup.
    BuiltinIter { id: u64, kind: BuiltinIterName },
    /// A type object — `type(x)` for a built-in type, or a built-in type name.
    /// Carries the type name surfaced by `.__name__` and `repr` (`<class 'int'>`).
    Type(String),
    /// A user-defined class object. Carries only the class name; the methods and
    /// class attributes live in the interpreter state's class
    /// registry keyed by that name, so the handle is cheap to clone and an
    /// instance never has to copy its class's methods.
    Class(String),
    /// An imported module namespace (`math`, `json`, …). Carries the module
    /// name; attribute and function access resolve against the module registry.
    Module(String),
    /// An instance of a user-defined class.
    Instance(InstanceValue),
    /// `functools.partial(func, *args, **kwargs)` — a callable that
    /// forwards to `func` with the bound positional / keyword args
    /// prepended/merged with the call's own. CPython exposes `.func`,
    /// `.args`, `.keywords` attributes; we expose the same.
    ///
    /// Boxed via [`PartialData`] so the inline footprint is one pointer
    /// (~8 B) instead of the ~80 B of the args + keywords containers;
    /// `Partial` is one of the rarer variants and gating Value enum size
    /// on it hurts every other clone / push / match.
    Partial(Box<PartialData>),
    /// A callable produced by `operator.itemgetter` / `attrgetter` /
    /// `methodcaller`. Boxed to keep the inline slot narrow; the call path
    /// applies it to its single argument.
    OperatorGetter(Box<OperatorGetter>),
    /// `functools.lru_cache`-wrapped callable. Shared interior state so
    /// clones share the memo table (CPython identity of the wrapper).
    LruCache(std::sync::Arc<LruCacheData>),
    /// A callable bound to a stdlib module function (`math.sqrt`, or a name
    /// pulled in via `from math import sqrt`). Carries the module and function
    /// names; the call path dispatches it against the module registry.
    ModuleFunction { module: String, name: String },
    /// A `datetime.date` value, backed by a chrono `NaiveDate` for correct
    /// calendar arithmetic and validation.
    Date(NaiveDate),
    /// A `datetime.datetime` value. Naive (no tzinfo) unless
    /// `tz_offset_secs` is `Some(_)`; aware values carry the fixed
    /// offset in seconds east of UTC. Aware vs naive arithmetic is
    /// enforced per CPython: mixing them raises TypeError. Offset
    /// stored as `i32` seconds rather than chrono `FixedOffset` because
    /// `FixedOffset` doesn't impl serde Serialize.
    DateTime { dt: NaiveDateTime, tz_offset_secs: Option<i32> },
    /// A `datetime.time` value. Always naive in our model;
    /// CPython supports tzinfo on time but it is rarely used.
    Time(NaiveTime),
    /// A `datetime.timedelta` value, backed by chrono
    /// `Duration` (microsecond-precision). Arithmetic with date /
    /// datetime through the legacy `apply_binop` path. Stored as raw
    /// microseconds because chrono `Duration` doesn't impl serde
    /// Serialize directly.
    TimeDelta(i64),
    /// A `datetime.timezone` value, holding a fixed UTC
    /// offset in seconds east of UTC. `datetime.timezone.utc` constant
    /// returns `TimeZone(0)`.
    TimeZone(i32),
    /// A `hashlib` hash-digest value. Carries the algorithm
    /// name (`sha256`/`sha512`) and the digest bytes. Methods
    /// `.hexdigest()` and `.digest()` round-trip the bytes.
    HashDigest { algo: String, bytes: Vec<u8> },
    /// `collections.deque` — double-ended queue. Backed by
    /// `VecDeque<Value>` so append/pop on either end are O(1). Method
    /// dispatch handles append, appendleft, pop, popleft, extend,
    /// extendleft, rotate, clear. Bounded form (`maxlen`) is modelled
    /// by trimming on push.
    Deque { items: std::collections::VecDeque<Self>, maxlen: Option<usize> },
    /// `collections.defaultdict` — dict that synthesises missing keys
    /// from a factory callable. Boxed via [`DefaultDictData`] so the
    /// inline Value enum slot stays narrow (the inline form was ~56 B,
    /// dominated by the `IndexMap`). `factory` is a stored callable
    /// Value (Function / Lambda / Class). Missing-key reads insert
    /// `factory()` under the key and return it — distinct from
    /// Counter's `__missing__` which returns 0 without inserting.
    DefaultDict(Box<DefaultDictData>),
    /// `enum.Enum` member. Wraps the underlying value with
    /// the class name + member name + kind (Plain vs IntEnum vs
    /// StrEnum), so we can match CPython's `Color.RED` repr,
    /// identity-based equality for plain Enum, and value-coercion
    /// equality / arithmetic for IntEnum / StrEnum.
    EnumMember { class_name: String, member_name: String, value: Box<Self>, kind: EnumKind },
    /// A regular-expression match object, returned by `re.match`/`search`/
    /// `fullmatch`. Supports `.group()`, `.groups()`, `.start()`, `.end()`,
    /// `.span()`, and `.groupdict()`.
    ///
    /// Boxed to keep `size_of::<Value>()` small — `MatchValue` is ~72 B
    /// inline (Vec + IndexMap), but `ReMatch` is one of the rarer
    /// variants. Storing it behind a `Box` shrinks every other
    /// Value enum slot (every clone, every push, every match arm) at the
    /// cost of one heap indirection for the rare re-match path.
    ReMatch(Box<MatchValue>),
    /// A compiled regular expression, returned by `re.compile(pattern)`.
    /// Carries the pattern source; its methods (`.match`, `.search`,
    /// `.fullmatch`, `.findall`, `.sub`, `.split`) delegate to the `re`
    /// module functions with the stored pattern as the leading argument,
    /// and `.pattern` reads the source back. Boxed to keep the enum slot
    /// narrow, mirroring `ReMatch`.
    RePattern(Box<String>),
    /// A `super()` proxy. Carries the defining class whose
    /// MRO slot we're resuming from plus the bound `self` instance.
    /// Method calls on a `Super` value walk the MRO starting from the
    /// slot *after* `defining_class` and dispatch the matching method
    /// against `instance` — that's how `super().method(...)` invokes
    /// the parent's implementation while staying on the original
    /// receiver.
    Super { defining_class: String, instance: Box<InstanceValue> },
    /// `collections.Counter`. Models CPython's
    /// `Counter(dict)` subclass: same key-value storage as `Dict` but
    /// with `__missing__` returning `0` (no insert), a distinct repr
    /// `Counter({...})`, and bespoke methods (most_common, elements,
    /// subtract, update, plus multiset arithmetic via +/-/&/|).
    Counter(
        #[serde(serialize_with = "serialize_dict", deserialize_with = "deserialize_dict")]
        IndexMap<ValueKey, Self>,
    ),
    /// `decimal.Decimal` — arbitrary-precision decimal arithmetic that
    /// matches CPython's exact-input/exact-output contract (no
    /// binary-float roundoff). Boxed because `BigDecimal` is large
    /// (~48 bytes) and would inflate every `Value` slot otherwise.
    Decimal(Box<bigdecimal::BigDecimal>),
    /// `fractions.Fraction` — auto-simplifying rational (numerator /
    /// denominator). `BigRational` keeps arbitrary precision so
    /// LCM-driven addition does not overflow. Boxed for the same
    /// `Value` size reason as `Decimal`.
    Fraction(Box<num_rational::BigRational>),
    /// A builtin method bound to its receiver — `d.get`, `s.upper`,
    /// `lst.append`, etc. Produced by attribute access on a builtin
    /// type when the attribute name resolves to a method. Callable from
    /// any callable slot (`key=d.get`, `map(str.upper, items)`,
    /// stored in a variable then invoked).
    ///
    /// Receiver carries either a snapshot or a place reference; see
    /// [`BoundMethodReceiver`] for the divergence trade-offs. Mutating
    /// methods captured from a navigable place (`push = xs.append`)
    /// propagate back to the original variable; receivers built from
    /// temporaries (`push = [1,2].append`) snapshot and discard
    /// mutations, matching CPython where the temp is unobservable.
    BoundMethod { receiver: BoundMethodReceiver, method: String },
    /// An unbound method on a builtin type — `str.upper`, `int.bit_length`,
    /// `list.append` (the descriptor form, *not* an instance binding).
    /// CPython models this as a type-level function descriptor that
    /// receives the instance as its first positional argument when
    /// called. Produced by attribute access on the `__builtin__<T>`
    /// name sentinel (the type, not an instance). On call, the first
    /// arg becomes the receiver and dispatch_method handles the rest.
    BuiltinTypeMethod { type_name: String, method: String },
    /// A bare-name reference to a CPython builtin function or type
    /// (`print`, `len`, `str`, `int`, ...). Produced by `eval_name`
    /// when an undefined identifier matches the builtin allowlist;
    /// consumed by `call_value_as_function`, the indirection
    /// dispatch path, and `try_builtin`. Carries just the builtin's
    /// name; resolution happens at call time.
    ///
    /// Replaces the earlier stringly-typed `Value::String("__builtin__X")`
    /// sentinel — a real user variable assigned `"__builtin__print"`
    /// no longer accidentally becomes callable.
    BuiltinName(String),
    /// A bare-name reference to a registered tool (`my_tool`). Same
    /// shape as `BuiltinName`; resolution goes through
    /// `tools::resolver::resolve_and_dispatch` at call time.
    /// Replaces the `__tool__X` sentinel string.
    ToolName(String),
    /// A bare-name reference to a CPython exception type
    /// (`ValueError`, `TypeError`, `KeyError`, ...). Produced by
    /// `eval_name` when an undefined identifier matches the
    /// exception-type allowlist. Constructing an instance
    /// (`ValueError("msg")`) goes through the call evaluator's
    /// exception-constructor fast path. Replaces the
    /// `__exception_type__X` sentinel string.
    ExceptionType(String),
    /// An unbound class method captured as a value (`Cls.method`
    /// where method is a `@classmethod` — staticmethods resolve to
    /// `Value::Function` directly, regular methods aren't capturable
    /// without a receiver). Dispatch routes through
    /// `classes::call_method` with the class as receiver at call
    /// time. Replaces the `__class_method__<class>__<method>`
    /// sentinel string (which had a rsplit_once collision risk when
    /// class names contained `__`).
    UnboundClassMethod { class: String, method: String },
}

/// The receiver of a [`Value::BoundMethod`].
///
/// Two shapes:
///
/// - `Snapshot` — a cloned value, captured at attribute-access time. Used when the receiver
///   expression is a temporary (literal, function-call result, comprehension) and no place
///   reference can exist. Mutations through this bound method affect the snapshot only, which
///   mirrors CPython: a temporary list mutated via a captured `.append` is unobservable to any
///   other code.
///
/// - `Place` — a root variable name plus accessor steps. The receiver is navigated against live
///   interpreter state at call time, so mutations propagate back to the original variable. This is
///   what makes `push = xs.append; push(1)` produce `xs == [1]` rather than the value-semantics
///   divergence we'd otherwise carry.
///
/// **Divergence**: Place reference is to a *variable name*, not to the
/// underlying object. If the variable is reassigned between capture
/// and call, our model dispatches against the *current* binding,
/// whereas CPython would still hold the original object. Accept for
/// the accumulator pattern, document elsewhere.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BoundMethodReceiver {
    Snapshot(Box<Value>),
    Place {
        /// Root variable name in `InterpreterState::variables`.
        root: String,
        /// Accessor chain navigated from the root to reach the slot.
        /// Empty for `name.method`; non-empty for `name[k].method`,
        /// `name.field.method`, etc.
        steps: Vec<BoundMethodStep>,
    },
}

/// One accessor in a [`BoundMethodReceiver::Place`] chain.
///
/// Mirrors the evaluator's place-step path minus the `Slice` variant,
/// which is non-navigable (CPython treats `lst[1:].append(x)` as
/// mutating a temporary). A separate enum, rather than reusing
/// `PlaceStep` directly, so the type system enforces "only navigable
/// steps live in a bound method".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BoundMethodStep {
    Index(Value),
    Attr(String),
}

/// Backing data for `Value::DefaultDict` — the entries + the missing-key
/// factory. Stored behind a `Box` on the `DefaultDict` variant so the
/// inline Value enum slot stays narrow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultDictData {
    #[serde(serialize_with = "serialize_dict", deserialize_with = "deserialize_dict")]
    pub items: IndexMap<ValueKey, Value>,
    /// Callable invoked to materialise a missing key.
    pub factory: Value,
}

/// Backing data for `Value::Partial` — the bound callable + its
/// captured positional / keyword args. Stored behind a `Box` on the
/// `Partial` variant so the inline Value enum slot stays narrow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialData {
    /// The callable being partially applied (Function / Lambda / Class /
    /// builtin handle). Owned by value.
    pub func: Value,
    /// Bound positional args, prepended on every dispatched call.
    pub args: Vec<Value>,
    /// Bound keyword args, merged into the dispatched call's kwargs.
    pub keywords: indexmap::IndexMap<String, Value>,
}

/// Backing data for [`Value::OperatorGetter`] — the three callable factories in
/// the `operator` module. Each applies to a single argument at call time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperatorGetter {
    /// `itemgetter(*items)` — `obj[item]`, or a tuple of them for 2+ items.
    ItemGetter(Vec<Value>),
    /// `attrgetter(*attrs)` — `obj.attr`, or a tuple; each attr may be a dotted
    /// path (`"a.b"`), stored pre-split into its components.
    AttrGetter(Vec<Vec<String>>),
    /// `methodcaller(name, *args, **kwargs)` — `obj.name(*args, **kwargs)`.
    MethodCaller { name: String, args: Vec<Value>, kwargs: indexmap::IndexMap<String, Value> },
}

/// Shared state for [`Value::LruCache`].
#[derive(Debug)]
pub struct LruCacheData {
    /// Wrapped callable.
    pub func: Value,
    /// Max entries; `None` = unbounded (CPython `maxsize=None`).
    pub maxsize: Option<usize>,
    /// Insertion-ordered memo: key = positional ValueKeys (kwargs unsupported).
    pub cache: Mutex<IndexMap<Vec<ValueKey>, Value>>,
}

// LruCache is process-local memo state — not restored from checkpoints.
impl Serialize for LruCacheData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = serializer.serialize_struct("LruCacheData", 2)?;
        st.serialize_field("func", &self.func)?;
        st.serialize_field("maxsize", &self.maxsize)?;
        st.end()
    }
}

impl<'de> Deserialize<'de> for LruCacheData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            func: Value,
            maxsize: Option<usize>,
        }
        let w = Wire::deserialize(deserializer)?;
        Ok(Self { func: w.func, maxsize: w.maxsize, cache: Mutex::new(IndexMap::new()) })
    }
}

/// A regex match: its capture groups (index 0 is the whole match) plus a
/// name→index map for named groups. Offsets are character indices, matching
/// Python's `str`-based `re`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchValue {
    /// Capture groups; index 0 is the whole match. `None` means the group did
    /// not participate in the match.
    pub groups: Vec<Option<MatchGroup>>,
    /// Named groups, mapping each name to its group index. Insertion-
    /// ordered (CPython's `groupdict()` preserves source order).
    pub named: indexmap::IndexMap<String, usize>,
}

/// One capture group of a [`MatchValue`]: its text and character span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchGroup {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// A `slice(start, stop, step)` object's bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceValue {
    /// Start bound — `Value::None` or `Value::Int`.
    pub start: Value,
    /// Stop bound — `Value::None` or `Value::Int`.
    pub stop: Value,
    /// Step — `Value::None` or `Value::Int`.
    pub step: Value,
}

/// A user-defined class instance: its class name plus its own attributes.
///
/// Methods are not stored here — they live in the class registry on
/// `InterpreterState`, looked up by `class_name` at call time.
///
/// `fields` is a shared map ([`SharedFields`]) so cloning an instance Value
/// preserves object identity for attribute mutation — same model as
/// [`SharedList`] for `list`. Map iteration order is deterministic
/// (`BTreeMap`); attribute order is not user-observable (`__dict__` /
/// `vars()` are not exposed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceValue {
    pub class_name: String,
    #[serde(serialize_with = "serialize_shared_fields")]
    #[serde(deserialize_with = "deserialize_shared_fields")]
    pub fields: SharedFields,
}

/// The definition of a user-defined class, held in the interpreter's class
/// registry (not as a `Value` — variables hold a lightweight [`Value::Class`]
/// handle that names this entry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassValue {
    pub name: String,
    /// Method definitions keyed by method name. Each `FunctionDef::name` is the
    /// qualified `Class.method` key under which its body AST is cached in
    /// `InterpreterState::function_bodies`.
    pub methods: BTreeMap<String, FunctionDef>,
    /// Class-level attributes (`class C: x = 1`), shared by all instances until
    /// shadowed by an instance attribute.
    pub class_attrs: BTreeMap<String, Value>,
    /// Direct base classes in declaration order (B1). Empty for classes
    /// that declare no explicit bases — `object` is implicit and not
    /// added here so the registry stays simple (it is not a real
    /// registered class).
    pub bases: Vec<String>,
    /// C3-linearized method resolution order, including `self` at index
    /// 0 and excluding the implicit `object` tail. Attribute and method
    /// lookups walk this order; `super()` resumes from the slot after
    /// the calling class. Computed once at class-definition time.
    pub mro: Vec<String>,
    /// `@property` data descriptors (B2). Attribute lookup checks
    /// `properties` before instance fields, matching CPython's
    /// data-descriptor precedence over instance dict.
    pub properties: BTreeMap<String, PropertyDef>,
    /// `@staticmethod` methods. Called without binding `self`.
    pub static_methods: BTreeMap<String, FunctionDef>,
    /// `@classmethod` methods. Called with the class as the first arg.
    pub class_methods: BTreeMap<String, FunctionDef>,
    /// `Some(kind)` if this class derives from `enum.Enum` /
    /// `IntEnum` / `StrEnum`. Set at class-definition time when one
    /// of the bases resolves to an enum sentinel; drives class-body
    /// value wrapping (raw `RED = 1` becomes `Color.RED` enum member).
    #[serde(default)]
    pub enum_kind: Option<EnumKind>,
    /// Enum member names in class-body declaration order. Drives iteration
    /// (`for m in Color`) and `list(Color)`, which CPython yields in definition
    /// order — class_attrs is a BTreeMap and would sort them alphabetically.
    #[serde(default)]
    pub enum_members: Vec<String>,
    /// Annotated attribute names in class-body declaration order. Populated
    /// by every `name: Type` line (with or without a value). Drives the
    /// `@dataclass` decorator's field discovery — class_attrs is alphabetical
    /// (BTreeMap) and would scramble the `__init__` parameter order.
    #[serde(default)]
    pub annotations: Vec<String>,
    /// `Some(fields)` if the class has been processed by the `@dataclass`
    /// decorator. Each field carries its name, optional default, and the
    /// per-field flags (init / repr / compare). Drives synthesized
    /// `__init__`, `__repr__`, and `__eq__` behaviour at instance-time.
    #[serde(default)]
    pub dataclass_fields: Option<Vec<DataclassField>>,
    /// `@dataclass(frozen=True)` — instance field writes raise FrozenInstanceError.
    #[serde(default)]
    pub frozen: bool,
    /// `@dataclass(order=True)` — rich comparisons use field tuples.
    #[serde(default)]
    pub order: bool,
    /// `@dataclass(slots=True)` or class-body `__slots__` — only listed
    /// fields may be set on instances (CPython's no-`__dict__` restriction,
    /// modelled as a field-name allowlist rather than layout change).
    #[serde(default)]
    pub slots: bool,
    /// Names allowed when `slots` is true (from `__slots__` or dataclass fields).
    #[serde(default)]
    pub slot_names: Vec<String>,
}

impl ClassValue {
    /// Empty class shell with safe defaults (no methods/attrs/slots).
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            methods: BTreeMap::new(),
            class_attrs: BTreeMap::new(),
            bases: Vec::new(),
            mro: vec![name],
            properties: BTreeMap::new(),
            static_methods: BTreeMap::new(),
            class_methods: BTreeMap::new(),
            enum_kind: None,
            enum_members: Vec::new(),
            annotations: Vec::new(),
            dataclass_fields: None,
            frozen: false,
            order: false,
            slots: false,
            slot_names: Vec::new(),
        }
    }
}

/// A single field of an `@dataclass`-decorated class.
///
/// The boolean flags mirror CPython's `dataclasses.field(...)` kwargs:
/// `repr=False` excludes the field from the synthesized `__repr__`,
/// `compare=False` excludes it from `__eq__`, `init=False` excludes it
/// from the synthesized `__init__` parameter list (in which case the
/// `default` is applied unconditionally).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataclassField {
    pub name: String,
    /// `Some(value)` if a default is supplied as a literal in the class
    /// body (`x: int = 5`) or via `field(default=...)`. Mutually
    /// exclusive with `default_factory` at the CPython level — both set
    /// is a `ValueError`.
    pub default: Option<Value>,
    /// `Some(callable)` if the field uses `field(default_factory=fn)` —
    /// the factory is invoked on every `__init__` that does not supply
    /// the field, producing a fresh value (the idiomatic way to default
    /// to a mutable container like `list` or `dict`).
    pub default_factory: Option<Value>,
    pub init: bool,
    pub repr: bool,
    pub compare: bool,
}

/// Enum kind drives EnumMember equality + arithmetic semantics.
///
/// CPython's plain `Enum` is identity-based: `Color.RED == 1` is
/// False. `IntEnum` inherits from int so `Priority.LOW + Priority.HIGH`
/// is int arithmetic. `StrEnum` similarly mixes with str.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnumKind {
    Plain,
    Int,
    Str,
}

/// A `@property` data descriptor.
///
/// Required getter plus optional setter and deleter. Reads call
/// `getter`; writes call `setter` (`AttributeError` if absent);
/// `del inst.x` calls `deleter` (`AttributeError` if absent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyDef {
    pub getter: FunctionDef,
    pub setter: Option<FunctionDef>,
    pub deleter: Option<FunctionDef>,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::BigInt(a), Self::BigInt(b)) => a == b,
            (Self::Int(a), Self::BigInt(b)) | (Self::BigInt(b), Self::Int(a)) => {
                b.as_ref() == &num_bigint::BigInt::from(*a)
            }
            (Self::Bool(b), Self::BigInt(i)) | (Self::BigInt(i), Self::Bool(b)) => {
                i.as_ref() == &num_bigint::BigInt::from(i64::from(*b))
            }
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            // String compares its inner CompactString; type/class/module compare
            // their inner String names.
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Type(a), Self::Type(b))
            | (Self::Class(a), Self::Class(b))
            | (Self::Module(a), Self::Module(b)) => a == b,
            (Self::Bytes(a), Self::Bytes(b)) => a == b,
            // bytearray compares equal to bytes with the same contents.
            (Self::ByteArray(a), Self::ByteArray(b)) => *a.lock() == *b.lock(),
            (Self::ByteArray(a), Self::Bytes(b)) | (Self::Bytes(b), Self::ByteArray(a)) => {
                *a.lock() == *b
            }
            // List is shared via Arc<Mutex<Vec>> — `Arc::ptr_eq` short-
            // circuits the lock acquisition when two aliases hold the
            // same backing storage (the common `a = b = []` case).
            // Otherwise lock both and compare element-wise.
            (Self::List(a), Self::List(b)) => {
                if Arc::ptr_eq(a, b) {
                    return true;
                }
                let a_guard = a.lock();
                let b_guard = b.lock();
                a_guard.len() == b_guard.len()
                    && a_guard.iter().zip(b_guard.iter()).all(|(x, y)| x == y)
            }
            (Self::Tuple(a), Self::Tuple(b)) | (Self::Set(a), Self::Set(b)) => a == b,
            // set/frozenset equality is order-independent and cross-type, so
            // `frozenset([1, 2])` keyed structurally still matches `{2, 1}`.
            (Self::Frozenset(a), Self::Frozenset(b))
            | (Self::Frozenset(a), Self::Set(b))
            | (Self::Set(a), Self::Frozenset(b)) => {
                a.len() == b.len() && a.iter().all(|x| b.contains(x))
            }
            (Self::Dict(a), Self::Dict(b)) => a == b,
            (
                Self::Range { start: s1, stop: e1, step: st1 },
                Self::Range { start: s2, stop: e2, step: st2 },
            ) => s1 == s2 && e1 == e2 && st1 == st2,
            (Self::Exception(a), Self::Exception(b)) => {
                a.type_name == b.type_name && a.message == b.message
            }
            (Self::Date(a), Self::Date(b)) => a == b,
            (
                Self::ModuleFunction { module: m1, name: n1 },
                Self::ModuleFunction { module: m2, name: n2 },
            ) => m1 == m2 && n1 == n2,
            // EnumMember vs EnumMember: identity-based — same class
            // AND same member name.
            (
                Self::EnumMember { class_name: c1, member_name: m1, .. },
                Self::EnumMember { class_name: c2, member_name: m2, .. },
            ) => c1 == c2 && m1 == m2,
            // IntEnum / StrEnum vs raw int/str: compare underlying
            // value. Plain Enum never equates to a raw literal.
            (Self::EnumMember { value, kind: EnumKind::Int | EnumKind::Str, .. }, other) => {
                value.as_ref() == other
            }
            (other, Self::EnumMember { value, kind: EnumKind::Int | EnumKind::Str, .. }) => {
                other == value.as_ref()
            }
            // Decimal / Fraction equality is value-based and exact —
            // both BigDecimal and BigRational normalise on construction
            // so identical mathematical values compare equal even when
            // produced by different arithmetic paths.
            (Self::Decimal(a), Self::Decimal(b)) => a == b,
            (Self::Fraction(a), Self::Fraction(b)) => a == b,
            // Decimal == int: lift int into Decimal and compare; matches
            // CPython where `Decimal(5) == 5 is True`.
            (Self::Decimal(d), Self::Int(i)) | (Self::Int(i), Self::Decimal(d)) => {
                d.as_ref() == &bigdecimal::BigDecimal::from(*i)
            }
            (Self::Decimal(d), Self::BigInt(i)) | (Self::BigInt(i), Self::Decimal(d)) => {
                d.as_ref() == &bigdecimal::BigDecimal::from(i.as_ref().clone())
            }
            // Fraction == int / Fraction == Fraction-of-int: lift int
            // into a denom=1 Rational.
            (Self::Fraction(f), Self::Int(i)) | (Self::Int(i), Self::Fraction(f)) => {
                f.as_ref() == &num_rational::BigRational::from_integer(num_bigint::BigInt::from(*i))
            }
            (Self::Fraction(f), Self::BigInt(i)) | (Self::BigInt(i), Self::Fraction(f)) => {
                f.as_ref() == &num_rational::BigRational::from_integer(i.as_ref().clone())
            }
            // Functions, lambdas, proxies, and instances (no __eq__ / identity)
            // are never equal.
            _ => false,
        }
    }
}

/// Dict keys must be hashable — a subset of Value.
///
/// `Ord` is derived for `json.dumps(sort_keys=True)`, which orders by the
/// original key (e.g. int keys `1, 2, 10` numerically) rather than by their
/// stringified form. Homogeneous keys (all-int, all-str) sort as CPython does;
/// mixed-type keys (which CPython rejects) get a deterministic variant order.
///
/// `PartialEq`/`Eq`/`Hash` are HAND-IMPLEMENTED below so `Bool(true)` and
/// `Int(1)` compare equal and hash to the same bucket — CPython's
/// bool-is-int-subclass semantics for dict keys. The variants stay distinct
/// at the storage level so downstream consumers (e.g. `json.dumps`) can read
/// the original type off the stored key, matching CPython's "first-inserted
/// key object wins" dict behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ValueKey {
    None,
    /// The `Ellipsis` singleton as a dict/set key.
    Ellipsis,
    Bool(bool),
    Int(i64),
    /// Arbitrary-precision int key (outside i64).
    BigInt(num_bigint::BigInt),
    /// Non-integral float key, stored as raw IEEE-754 bits so the key derives
    /// `Eq`/`Hash` (which `f64` cannot). Integral floats never reach this
    /// variant: dict-key coercion folds `2.0` into
    /// `Int(2)` so that `{2: x}[2.0]` hits the same slot, matching CPython's
    /// `hash(2.0) == hash(2)` numeric-key unification.
    Float(u64),
    /// Non-real `complex` key: raw IEEE-754 bits of `(real, imag)`. A real
    /// complex (`imag == 0`) never reaches this variant — it folds to the
    /// float/int key in `value_to_key`, so `{1, 1+0j}` dedups and complex
    /// keys share slots with equal ints/floats.
    Complex(u64, u64),
    /// String dict key. Same SSO rationale as [`Value::String`] —
    /// inline up to 24 B, spill to heap beyond.
    String(CompactString),
    Tuple(Vec<Self>),
    /// `frozenset` key. Element order is irrelevant to equality and hash
    /// (two frozensets are equal iff they hold the same elements), so this
    /// variant hand-implements set-equality in `PartialEq` and an
    /// order-independent hash.
    Frozenset(Vec<Self>),
    /// User-class instance key. `hash` is the precomputed value from
    /// the class's `__hash__` slot (called once at key-construction
    /// time at the async eval-layer boundary); `value` carries the
    /// original Instance so equality comparisons can run structurally.
    /// Equality on this variant uses `values_equal`, which works for
    /// classes whose `__eq__` is field-by-field — tracked by
    /// `gap-instance-dict-key-equality-dunder-parity`
    /// for classes whose `__eq__` diverges from structural equality
    /// (e.g. case-insensitive string wrappers).
    Instance {
        hash: i64,
        value: Box<Value>,
    },
}

impl PartialEq for ValueKey {
    fn eq(&self, other: &Self) -> bool {
        // Numeric equivalence: Bool(b) == Int(b as i64). Float keys only
        // reach the Float variant when non-integral, so they're never
        // cross-equal with Int / Bool (the integer-valued float fold in
        // `value_to_key` happens before construction).
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Ellipsis, Self::Ellipsis) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Bool(b), Self::Int(i)) | (Self::Int(i), Self::Bool(b)) => *i == i64::from(*b),
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::BigInt(a), Self::BigInt(b)) => a == b,
            (Self::Int(a), Self::BigInt(b)) | (Self::BigInt(b), Self::Int(a)) => {
                b == &num_bigint::BigInt::from(*a)
            }
            (Self::Bool(b), Self::BigInt(i)) | (Self::BigInt(i), Self::Bool(b)) => {
                i == &num_bigint::BigInt::from(i64::from(*b))
            }
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::Complex(ar, ai), Self::Complex(br, bi)) => ar == br && ai == bi,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Tuple(a), Self::Tuple(b)) => a == b,
            // Set equality: same cardinality and every element of one is in
            // the other. Order-independent, matching Python's frozenset.
            (Self::Frozenset(a), Self::Frozenset(b)) => {
                a.len() == b.len() && a.iter().all(|x| b.contains(x))
            }
            // Instance keys compare by IDENTITY, not structurally. Two
            // distinct instances are distinct dict/set keys even when
            // their fields compare equal — e.g. two bare `object()`
            // sentinels. Value-equality dedup for keys with a custom
            // `__eq__`/`__hash__` is done on the async dict path
            // (`dict_insert_instance_key` / `dict_get_instance_key`)
            // BEFORE a key is ever handed to the map, so this sync `Eq`
            // must only collapse the *same* instance. A structural
            // compare here silently merged identity-distinct instances
            // whenever their address-hashes shared a hashbrown control
            // byte (~1/128), corrupting `len`/lookup non-deterministically.
            (Self::Instance { value: a, .. }, Self::Instance { value: b, .. }) => {
                match (a.as_ref(), b.as_ref()) {
                    (Value::Instance(ia), Value::Instance(ib)) => {
                        std::sync::Arc::ptr_eq(&ia.fields, &ib.fields)
                    }
                    // Instance keys always box an `Instance`; the
                    // structural fallback is defensive only.
                    _ => crate::eval::operations::values_equal_pub(a, b),
                }
            }
            _ => false,
        }
    }
}

impl Eq for ValueKey {}

impl core::hash::Hash for ValueKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        // Hash MUST agree with PartialEq above: Bool(b) and Int(b as i64)
        // hash to the same bucket so an IndexMap lookup with one finds an
        // entry inserted under the other. We hash through a fixed tag so
        // different variants don't collide on the same byte pattern (a
        // bare `bool` and an `i64` with the same machine word would
        // otherwise stand on each other's hash output).
        const NONE_TAG: u8 = 0;
        const NUMERIC_TAG: u8 = 1;
        const FLOAT_TAG: u8 = 2;
        const STRING_TAG: u8 = 3;
        const TUPLE_TAG: u8 = 4;
        const INSTANCE_TAG: u8 = 5;
        const COMPLEX_TAG: u8 = 6;
        const ELLIPSIS_TAG: u8 = 7;
        const FROZENSET_TAG: u8 = 8;
        match self {
            Self::None => NONE_TAG.hash(state),
            Self::Ellipsis => ELLIPSIS_TAG.hash(state),
            Self::Bool(b) => {
                NUMERIC_TAG.hash(state);
                i64::from(*b).hash(state);
            }
            Self::Int(i) => {
                NUMERIC_TAG.hash(state);
                i.hash(state);
            }
            Self::BigInt(i) => {
                // Prefer numeric tag when value fits i64 so it collides
                // with Int/Bool keys of the same magnitude.
                NUMERIC_TAG.hash(state);
                if let Ok(n) = i64::try_from(i) {
                    n.hash(state);
                } else {
                    i.hash(state);
                }
            }
            Self::Float(bits) => {
                FLOAT_TAG.hash(state);
                bits.hash(state);
            }
            Self::Complex(re, im) => {
                COMPLEX_TAG.hash(state);
                re.hash(state);
                im.hash(state);
            }
            Self::String(s) => {
                STRING_TAG.hash(state);
                s.hash(state);
            }
            Self::Tuple(items) => {
                TUPLE_TAG.hash(state);
                items.hash(state);
            }
            Self::Frozenset(items) => {
                FROZENSET_TAG.hash(state);
                // Order-independent: XOR each element's individual hash so
                // permutations collide, agreeing with the set-equality above.
                let mut acc: u64 = 0;
                for item in items {
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    item.hash(&mut h);
                    acc ^= core::hash::Hasher::finish(&h);
                }
                acc.hash(state);
            }
            Self::Instance { hash, .. } => {
                INSTANCE_TAG.hash(state);
                hash.hash(state);
            }
        }
    }
}

/// A runtime exception value that can be raised and caught by user code.
///
/// `args` holds the positional constructor arguments (CPython's
/// `e.args`). For exceptions constructed via the user-facing
/// constructor (`ValueError('msg')`), this is populated from the
/// call args; for internally-raised exceptions (KeyError on dict
/// miss, IndexError on out-of-range subscript) it defaults to
/// empty and `exception_attribute.args` synthesizes `(message,)`
/// to match CPython's auto-arg behaviour.
///
/// `stamped_line` is set by `stamp_line` at the eval_stmt boundary
/// and rendered ONLY at the `Interpreter::execute` boundary into
/// the user-facing `errorMessage` — it is deliberately invisible
/// to `str(e)` / `repr(e)` / `print(f'{e}')` inside the script, so
/// the agent-loop debug suffix doesn't bleed into user code that
/// catches and inspects exceptions.
///
/// Constructed via [`ExceptionValue::new`] + the `with_*` chain —
/// the struct fields are public for the rare consumer that wants to
/// pattern-destructure, but new construction sites should not use
/// struct-literal form.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExceptionValue {
    pub type_name: String,
    pub message: String,
    pub cause: Option<Box<Self>>,
    #[serde(default)]
    pub args: Vec<Value>,
    #[serde(default)]
    pub stamped_line: Option<u32>,
    /// Nested exceptions for `ExceptionGroup` / `BaseExceptionGroup` (PEP 654).
    #[serde(default)]
    pub exceptions: Option<Vec<Self>>,
    /// Custom instance attributes set by a user exception's `__init__`
    /// (`self.code = ...`), preserved so `except E as e: e.code` works.
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
}

impl ExceptionValue {
    /// Build the standard `<Type>: <message>` exception with no
    /// cause, no line stamp. The 95% case.
    ///
    /// `args` mirrors CPython's positional-args behaviour: an empty
    /// message yields `args == ()` (matching `Exception()`), and a
    /// non-empty message yields `args == (message,)` (matching
    /// `Exception('msg')`). Multi-arg constructors and internal raisers
    /// that need a non-message arg layout call [`Self::with_args`] to
    /// override.
    #[must_use]
    pub fn new(type_name: impl Into<String>, message: impl Into<String>) -> Self {
        let message = message.into();
        let args = if message.is_empty() {
            Vec::new()
        } else {
            vec![Value::String(message.clone().into())]
        };
        Self {
            type_name: type_name.into(),
            message,
            cause: None,
            args,
            stamped_line: None,
            exceptions: None,
            fields: BTreeMap::new(),
        }
    }

    /// Build an `ExceptionGroup` (or `BaseExceptionGroup`) with nested exceptions.
    #[must_use]
    pub fn group(
        type_name: impl Into<String>,
        message: impl Into<String>,
        exceptions: Vec<Self>,
    ) -> Self {
        let message = message.into();
        let nested: Vec<Value> =
            exceptions.iter().cloned().map(|exc| Value::Exception(Box::new(exc))).collect();
        Self {
            type_name: type_name.into(),
            message: message.clone(),
            cause: None,
            args: vec![Value::String(message.into()), Value::List(shared_list(nested))],
            stamped_line: None,
            exceptions: Some(exceptions),
            fields: BTreeMap::new(),
        }
    }

    /// Attach a `raise X from Y`-style cause.
    #[must_use]
    pub fn with_cause(mut self, cause: Self) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }

    /// Set the constructor args. Used at the call-as-constructor
    /// path (`ValueError('msg', 'detail')`) so `e.args` reflects
    /// the exact values the user passed.
    #[must_use]
    pub fn with_args(mut self, args: Vec<Value>) -> Self {
        self.args = args;
        self
    }

    // --- Common-pattern shorthands ----------------------------------

    /// `KeyError(<key>)` — used by every dict/Counter/defaultdict
    /// miss. CPython's `KeyError` message is the key's repr.
    #[must_use]
    pub fn key_error(key: impl std::fmt::Display) -> Self {
        Self::new("KeyError", format!("{key}"))
    }

    /// `IndexError(<kind> index out of range)` — CPython varies the
    /// wording by container; pass the type-specific kind (`list`,
    /// `tuple`, `string`, `bytes`, `range object`, `deque`).
    #[must_use]
    pub fn index_error(kind: &str) -> Self {
        Self::new("IndexError", format!("{kind} index out of range"))
    }

    /// `ZeroDivisionError(division by zero)` — CPython's canonical
    /// wording for `1/0`.
    #[must_use]
    pub fn zero_division_error(message: impl Into<String>) -> Self {
        Self::new("ZeroDivisionError", message)
    }
}

/// Stored representation of a user-defined function (def).
/// Captures closure at DEFINITION time.
///
/// `source` is the original `def …:` text, carried on the struct so state
/// checkpoints round-trip without a side channel. The parsed body AST is
/// cached in `InterpreterState::function_bodies` keyed by `name` because
/// `rustpython_parser::ast` types are not `Serialize`/`Deserialize`; the
/// cache is populated at definition time and re-populated on
/// [`crate::Interpreter::import_state`] by re-parsing `source`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub params: FunctionParams,
    pub closure: BTreeMap<String, Value>,
    /// Original Python source for the `def` — re-parsed on state import
    /// to rebuild the body cache.
    pub source: String,
    /// Names declared `nonlocal` in the body. Mutations to these names
    /// inside the function propagate to the cell keyed by
    /// `nonlocal_cell_id` so subsequent calls see the updated values.
    /// Empty for functions that don't use `nonlocal`.
    #[serde(default)]
    pub nonlocal_names: Vec<String>,
    /// Computed at function-def time: `true` when the body contains a
    /// `yield` or `yield from` expression. Caches the result of
    /// `contains_yield_stmts` so `call_user_function` doesn't re-walk
    /// the body on every call. Old state imports default to `false`;
    /// the call path falls back to the dynamic walk in that case.
    #[serde(default)]
    pub is_generator: bool,
    /// `__name__` override set by `functools.wraps`. When `Some`, attribute
    /// access reports this instead of `name` (which stays the body-cache key,
    /// so the wrapper still dispatches correctly).
    #[serde(default)]
    pub wraps_name: Option<String>,
    /// Cell id for the shared `nonlocal` storage, allocated at
    /// definition time when `nonlocal_names` is non-empty. The cell
    /// lives in `InterpreterState::nonlocal_cells`; all Value::Function
    /// clones for this def share the same id, so multiple calls
    /// observe each other's mutations. `None` for functions without
    /// `nonlocal`.
    #[serde(default)]
    pub nonlocal_cell_id: Option<u64>,
    /// Names this function body assigns to (via `=`, `+=`, `for x in`,
    /// `except as`, `with ... as`, `import x`, `def`, `class`, `del`).
    /// Used at call time by the `VariableCheckpoint` to snapshot only
    /// the names the frame can touch, rather than cloning the entire
    /// `state.variables` HashMap per frame. Walked statically by
    /// `collect_assigned_names` at function-definition time.
    /// Excludes names declared `global` (those persist to the
    /// enclosing scope) and `nonlocal` (those route through the cell
    /// pattern). Empty for functions whose bodies introduce no
    /// bindings — the checkpoint is then just the parameter set.
    #[serde(default)]
    pub assigned_names: Vec<String>,
    /// Names declared `global` in the body. Assignments to these names
    /// inside the function persist to the module (outer) scope and
    /// MUST NOT be restored by the per-frame checkpoint. Walked
    /// statically alongside `assigned_names`.
    #[serde(default)]
    pub global_names: Vec<String>,
    /// True when this function was defined at module scope (no
    /// enclosing function frame on the call stack at def time). At
    /// frame entry, the closure overlay is suppressed for names that
    /// are currently present in `state.variables` — those are module
    /// globals, and CPython's LEGB lookup reads the live module dict
    /// for free names, not a def-time snapshot. For nested defs
    /// (inside a function or class body) `is_module_level` is false
    /// and the closure overlay continues to win for closure-captured
    /// names — preserving the decorator-stack pattern where multiple
    /// wrappers share a parameter name and each must see its own
    /// captured value.
    #[serde(default)]
    pub is_module_level: bool,
}

/// Stored representation of a lambda.
///
/// Closure captured at definition time (matches Python's
/// late-binding-by-name semantics: looking up `x` inside the lambda
/// finds the binding from the enclosing scope at def time). Without
/// this, `adder = lambda x: lambda y: x + y; add5 = adder(5);
/// add5(3)` fails because the inner lambda can't see `x` after
/// `adder` has returned. The body AST is held in
/// `InterpreterState::lambda_bodies` keyed by `lambda_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaDef {
    pub params: FunctionParams,
    /// Key into `InterpreterState::lambda_bodies`. Generated when the lambda
    /// is evaluated for the first time.
    pub lambda_id: String,
    /// The original `lambda <params>: <body>` source text. Mirrors
    /// [`FunctionDef::source`] — re-parsed on state import to rebuild
    /// the lambda_bodies cache. Without this field, lambdas held in
    /// variables silently became uncallable after a `import_state`
    /// round-trip (the lambda_bodies map was reset and there was no
    /// repopulation path).
    pub source: String,
    /// Closure snapshot — variables captured from the enclosing scope
    /// at definition time. Layered under the parameter scope at call
    /// time so the lambda body sees free names from its definition
    /// site, even after the enclosing function has returned.
    #[serde(default)]
    pub closure: BTreeMap<String, Value>,
    /// Names this lambda body assigns to via walrus (`:=`). Used by
    /// `VariableCheckpoint`; mirrors `FunctionDef::assigned_names`.
    /// Lambda bodies are expressions, so the only binding form is the
    /// walrus operator; in most lambdas this list is empty.
    #[serde(default)]
    pub assigned_names: Vec<String>,
    /// True when this lambda was defined at module scope. Mirrors
    /// `FunctionDef::is_module_level` — same closure-overlay
    /// suppression rule applies.
    #[serde(default)]
    pub is_module_level: bool,
}

/// Function parameter specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionParams {
    /// Positional parameters.
    pub args: Vec<Param>,
    /// Default value expression source strings, retained for two
    /// reasons: (1) state imports older than the def-time evaluation
    /// landing fall back to re-parsing these at call time;
    /// (2) re-parsing is the only path available when no live
    /// evaluator was on hand at construction (e.g. synthesized
    /// methods built without a state reference).
    pub defaults: Vec<String>,
    /// Default values evaluated at def time. Populated whenever
    /// `eval_function_def` / `eval_lambda_def` has access to the
    /// state and tools — CPython evaluates defaults once at def
    /// time and reuses the same value per call (the mutable-default
    /// gotcha + the canonical `i=i` loop-capture idiom both depend
    /// on this). When the Vec is empty (e.g. on imported state from
    /// an older blob version), `bind_params` falls back to
    /// re-parsing `defaults` source strings.
    #[serde(default)]
    pub default_values: Vec<Value>,
    /// *args parameter name.
    pub vararg: Option<String>,
    /// Keyword-only parameters (after *).
    pub kwonlyargs: Vec<Param>,
    /// Keyword-only default value source strings. `None` marks a required
    /// keyword-only argument (no default).
    pub kw_defaults: Vec<Option<String>>,
    /// Same shape as `default_values` but for keyword-only params.
    /// `None` marks a required keyword-only argument.
    #[serde(default)]
    pub kw_default_values: Vec<Option<Value>>,
    /// **kwargs parameter name.
    pub kwarg: Option<String>,
}

/// A single function parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
}

impl Value {
    /// Check truthiness (Python semantics).
    #[inline]
    #[must_use]
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::None | Self::NotImplemented => false,
            // `bool(...)` is True — Ellipsis is a truthy singleton.
            Self::Ellipsis => true,
            Self::Bool(b) => *b,
            Self::Int(i) => *i != 0,
            Self::BigInt(i) => {
                use num_traits::Zero as _;
                !i.is_zero()
            }
            Self::Float(f) => *f != 0.0,
            Self::String(s) => !s.is_empty(),
            Self::Bytes(b) => !b.is_empty(),
            Self::ByteArray(b) => !b.lock().is_empty(),
            Self::MemoryView(inner) => inner.is_truthy(),
            Self::List(l) => !l.lock().is_empty(),
            Self::Tuple(t) => !t.is_empty(),
            Self::Dict(d) => !d.is_empty(),
            Self::Set(s) | Self::Frozenset(s) => !s.is_empty(),
            // Always truthy: callables, exceptions, proxies, type/class/module
            // objects, dates, match objects. (An instance is truthy unless it
            // defines `__bool__`/`__len__`; those aren't consulted in this
            // synchronous accessor, so it defaults to true.)
            Self::Function(_)
            | Self::Lambda(_)
            | Self::Exception(_)
            | Self::ExceptionMethod { .. }
            | Self::LazyProxy(_)
            | Self::Type(_)
            | Self::Class(_)
            | Self::Module(_)
            | Self::Instance(_)
            | Self::ModuleFunction { .. }
            | Self::Date(_)
            | Self::ReMatch(_)
            | Self::RePattern(_)
            | Self::Slice(_)
            | Self::Super { .. }
            | Self::DateTime { .. }
            | Self::Time(_)
            | Self::TimeZone(_)
            | Self::HashDigest { .. }
            | Self::BoundMethod { .. }
            | Self::BuiltinTypeMethod { .. }
            | Self::BuiltinName(_)
            | Self::ToolName(_)
            | Self::ExceptionType(_)
            | Self::UnboundClassMethod { .. }
            | Self::Lazy { .. }
            | Self::Generator { .. }
            | Self::BuiltinIter { .. }
            | Self::Partial { .. }
            | Self::OperatorGetter(_)
            | Self::LruCache(_) => true,
            // Counter, TimeDelta: zero is falsy (matches CPython's
            // `bool(timedelta(0))` being False).
            Self::Counter(c) => !c.is_empty(),
            Self::TimeDelta(micros) => *micros != 0,
            Self::Deque { items, .. } => !items.is_empty(),
            Self::DefaultDict(data) => !data.items.is_empty(),
            Self::EnumMember { value, .. } => value.is_truthy(),
            // Decimal / Fraction are falsy at zero, matching CPython
            // (`bool(Decimal("0")) is False`, `bool(Fraction(0)) is
            // False`).
            Self::Decimal(d) => !d.is_zero(),
            Self::Fraction(f) => !f.numer().is_zero(),
            // `bool(complex)` is False only when both parts are zero.
            Self::Complex(c) => c.re != 0.0 || c.im != 0.0,
            Self::Range { start, stop, step } => {
                if *step > 0 {
                    start < stop
                } else {
                    start > stop
                }
            }
        }
    }

    /// Get the Python type name for this value.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::OperatorGetter(g) => match **g {
                OperatorGetter::ItemGetter(_) => "itemgetter",
                OperatorGetter::AttrGetter(_) => "attrgetter",
                OperatorGetter::MethodCaller { .. } => "methodcaller",
            },
            Self::None => "NoneType",
            Self::NotImplemented => "NotImplementedType",
            Self::Ellipsis => "ellipsis",
            Self::Bool(_) => "bool",
            Self::Int(_) | Self::BigInt(_) => "int",
            Self::Float(_) => "float",
            Self::Complex(_) => "complex",
            Self::String(_) => "str",
            Self::Bytes(_) => "bytes",
            Self::ByteArray(_) => "bytearray",
            Self::MemoryView(_) => "memoryview",
            Self::List(_) => "list",
            Self::Tuple(_) => "tuple",
            Self::Dict(_) => "dict",
            Self::Set(_) => "set",
            Self::Frozenset(_) => "frozenset",
            // Both Function (named def) and Lambda (anonymous) are "function"
            // in Python's type system.
            Self::Function(_) | Self::Lambda(_) => "function",
            Self::Range { .. } => "range",
            Self::Exception(_) => "Exception",
            Self::ExceptionMethod { .. } => "method",
            Self::LazyProxy(_) => "LazyProxy",
            // A type object, a user class, and a builtin exception type
            // are all instances of `type`.
            Self::Type(_) | Self::Class(_) | Self::ExceptionType(_) => "type",
            Self::Module(_) => "module",
            // Generic name for instances; the concrete class name is exposed via
            // `python_type_name` where it matters (errors, `type()`).
            Self::Instance(_) => "object",
            Self::ModuleFunction { .. }
            | Self::BoundMethod { .. }
            | Self::BuiltinTypeMethod { .. }
            | Self::BuiltinName(_)
            | Self::ToolName(_)
            | Self::UnboundClassMethod { .. } => "builtin_function_or_method",
            Self::Date(_) => "date",
            Self::ReMatch(_) => "re.Match",
            Self::RePattern(_) => "re.Pattern",
            Self::Slice(_) => "slice",
            Self::Super { .. } => "super",
            Self::Counter(_) => "Counter",
            Self::DateTime { .. } => "datetime",
            Self::Time(_) => "time",
            Self::TimeDelta(_) => "timedelta",
            Self::TimeZone(_) => "timezone",
            Self::HashDigest { .. } => "_hashlib.HASH",
            Self::Deque { .. } => "deque",
            Self::DefaultDict { .. } => "defaultdict",
            // CPython: type(Color.RED).__name__ == "Color". Our model
            // returns the class name so `type(x).__name__` reflects
            // the enum class.
            Self::EnumMember { .. } => "enum",
            Self::Decimal(_) => "Decimal",
            Self::Fraction(_) => "Fraction",
            Self::Lazy { .. } | Self::Generator { .. } => "generator",
            Self::BuiltinIter { kind, .. } => kind.type_name(),
            Self::Partial { .. } => "functools.partial",
            Self::LruCache(_) => "functools._lru_cache_wrapper",
        }
    }

    /// The Python type name including the dynamic class name for instances.
    ///
    /// `type_name` returns a `&'static str` and so cannot carry an instance's
    /// class name; this owned variant does, for error messages and `type()`.
    #[must_use]
    pub fn python_type_name(&self) -> String {
        match self {
            Self::Instance(inst) => inst.class_name.clone(),
            Self::Type(n) | Self::Class(n) | Self::Module(n) => n.clone(),
            other => other.type_name().to_string(),
        }
    }
}

/// Format a float as CPython's `repr`/`str` does. Three places differ from
/// Rust's `{}`:
///   * non-finite values are lowercase `nan` / `inf` / `-inf` (Rust: `NaN`);
///   * integral finite values keep a trailing `.0` (`2.0`, not `2`);
///   * scientific notation kicks in at decimal exponent ≥ 16 or < −4 (`1e+16`, `1e-05`), with a
///     signed, ≥2-digit exponent — Rust's `{}` never switches to scientific.
///
/// Shared by `Value` and `ValueKey` Display.
/// Coerce a Counter entry value to i64 for sort ordering. Counter
/// stores Int values; bools coerce; other shapes (defensively) sort
/// as 0.
/// Write a tz offset in CPython's `+HH:MM` / `-HH:MM` shape.
fn write_tz_offset(f: &mut fmt::Formatter<'_>, secs: i32) -> fmt::Result {
    let sign = if secs < 0 { '-' } else { '+' };
    let abs = secs.unsigned_abs();
    let hours = abs / 3600;
    let minutes = (abs % 3600) / 60;
    write!(f, "{sign}{hours:02}:{minutes:02}")
}

/// Format microseconds as CPython's `timedelta` str. CPython:
///   timedelta(microseconds=7)            -> "0:00:00.000007"
///   timedelta(seconds=3)                 -> "0:00:03"
///   timedelta(days=1, seconds=10)        -> "1 day, 0:00:10"
///   timedelta(days=2, hours=3, minutes=4) -> "2 days, 3:04:00"
///   timedelta(microseconds=-1)           -> "-1 day, 23:59:59.999999"
///   (CPython normalises negative timedeltas so seconds/microseconds
///   stay non-negative.)
fn write_timedelta(f: &mut fmt::Formatter<'_>, micros: i64) -> fmt::Result {
    // CPython's timedelta uses a normalised representation where the
    // microsecond and second components are always non-negative. We
    // canonicalise here by dividing toward negative infinity (so
    // micros=-1 -> days=-1, secs=86399, us=999999).
    let secs_total = micros.div_euclid(1_000_000);
    let us = micros.rem_euclid(1_000_000);
    let days = secs_total.div_euclid(86_400);
    let day_remainder = secs_total.rem_euclid(86_400);
    let hours = day_remainder / 3600;
    let minutes = (day_remainder % 3600) / 60;
    let seconds = day_remainder % 60;

    if days != 0 {
        let suffix = if days == 1 || days == -1 { "" } else { "s" };
        write!(f, "{days} day{suffix}, ")?;
    }
    write!(f, "{hours}:{minutes:02}:{seconds:02}")?;
    if us != 0 {
        write!(f, ".{us:06}")?;
    }
    Ok(())
}

fn counter_value_as_i64(value: &Value) -> i64 {
    match value {
        Value::Int(n) => *n,
        Value::Bool(b) => i64::from(*b),
        _ => 0,
    }
}

fn write_python_float(f: &mut fmt::Formatter<'_>, v: f64) -> fmt::Result {
    if v.is_nan() {
        return write!(f, "nan");
    }
    if v.is_infinite() {
        return write!(f, "{}", if v > 0.0 { "inf" } else { "-inf" });
    }
    if v == 0.0 {
        // Preserve the sign of zero (`-0.0`).
        return write!(f, "{}", if v.is_sign_negative() { "-0.0" } else { "0.0" });
    }
    // Derive the decimal exponent from Rust's shortest scientific form, then
    // pick fixed vs. scientific the way CPython's `repr` does.
    let scientific = format!("{v:e}");
    let exponent: i32 = scientific.split_once('e').and_then(|(_, e)| e.parse().ok()).unwrap_or(0);
    if !(-4..16).contains(&exponent) {
        match scientific.split_once('e') {
            Some((mantissa, _)) => write!(f, "{mantissa}e{exponent:+03}"),
            None => write!(f, "{v}"),
        }
    } else if v.fract() == 0.0 {
        write!(f, "{v:.1}")
    } else {
        write!(f, "{v}")
    }
}

/// Format one component of a `complex` (the real or imaginary part). Same
/// fixed-vs-scientific rules as [`write_python_float`], but a whole number keeps
/// no trailing `.0` (`3.0` -> `"3"`) and zero is `"0"`/`"-0"` — matching
/// CPython's complex repr, which drops the `.0` a bare float keeps.
fn format_complex_component(v: f64) -> String {
    if v.is_nan() {
        return "nan".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf".to_string() } else { "-inf".to_string() };
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0".to_string() } else { "0".to_string() };
    }
    let scientific = format!("{v:e}");
    let exponent: i32 = scientific.split_once('e').and_then(|(_, e)| e.parse().ok()).unwrap_or(0);
    if !(-4..16).contains(&exponent) {
        match scientific.split_once('e') {
            Some((mantissa, _)) => format!("{mantissa}e{exponent:+03}"),
            None => format!("{v}"),
        }
    } else {
        // Rust's shortest form already drops a trailing `.0` (`3.0` -> `"3"`).
        format!("{v}")
    }
}

/// CPython's `repr(complex)` (identical to `str`): `"3j"` / `"(1+2j)"`. The
/// bare imaginary form is used only when the real part is a positive zero;
/// otherwise the parenthesised `(real±imagj)` form is used, with the sign taken
/// from the imaginary part's sign bit (so `-0.0` imag prints as `-0`).
fn format_complex(c: &num_complex::Complex64) -> String {
    if c.re == 0.0 && !c.re.is_sign_negative() {
        return format!("{}j", format_complex_component(c.im));
    }
    let re = format_complex_component(c.re);
    let neg = c.im.is_sign_negative() && !c.im.is_nan();
    let sign = if neg { "-" } else { "+" };
    let im = format_complex_component(c.im.abs());
    format!("({re}{sign}{im}j)")
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::NotImplemented => write!(f, "NotImplemented"),
            Self::Ellipsis => write!(f, "Ellipsis"),
            Self::Bool(true) => write!(f, "True"),
            Self::Bool(false) => write!(f, "False"),
            Self::Int(i) => write!(f, "{i}"),
            Self::BigInt(i) => write!(f, "{i}"),
            Self::Float(v) => write_python_float(f, *v),
            Self::Complex(c) => write!(f, "{}", format_complex(c)),
            Self::String(s) => write!(f, "{s}"),
            // CPython bytes repr — `b'...'` (or `b"..."` if the
            // content contains a single quote). Non-printable bytes,
            // backslash, and the chosen quote get escaped per the
            // CPython rules: `\\`, `\n`, `\r`, `\t`, and `\xNN` for
            // anything else outside the printable ASCII range.
            Self::Bytes(b) => write_bytes_literal(f, b),
            // CPython: `bytearray(b'abc')`.
            Self::ByteArray(b) => {
                write!(f, "bytearray(")?;
                write_bytes_literal(f, &b.lock())?;
                write!(f, ")")
            }
            Self::List(items) => {
                let snapshot = items.lock().clone();
                write!(f, "[")?;
                for (i, item) in snapshot.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item.repr())?;
                }
                write!(f, "]")
            }
            Self::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item.repr())?;
                }
                if items.len() == 1 {
                    write!(f, ",")?;
                }
                write!(f, ")")
            }
            Self::Dict(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v.repr())?;
                }
                write!(f, "}}")
            }
            Self::Set(items) => {
                write!(f, "{{")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item.repr())?;
                }
                write!(f, "}}")
            }
            // `frozenset({1, 2})`, or `frozenset()` when empty (CPython never
            // renders a bare `{}`, which is an empty dict).
            Self::Frozenset(items) => {
                if items.is_empty() {
                    return write!(f, "frozenset()");
                }
                write!(f, "frozenset({{")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item.repr())?;
                }
                write!(f, "}})")
            }
            Self::Function(fd) => write!(f, "<function {}>", fd.name),
            Self::Lambda(_) => write!(f, "<function <lambda>>"),
            Self::Range { start, stop, step } => {
                if *step == 1 {
                    write!(f, "range({start}, {stop})")
                } else {
                    write!(f, "range({start}, {stop}, {step})")
                }
            }
            // CPython: `str(ValueError('boom'))` -> `boom`, `repr(...)`
            // -> `ValueError('boom')`. Display IS str(); reserve the
            // typed repr for the repr() builtin.
            Self::Exception(e) => write!(f, "{}", e.message),
            Self::ExceptionMethod { method, exception } => {
                write!(f, "<bound method {method} of {}>", exception.type_name)
            }
            Self::LazyProxy(p) => write!(f, "<LazyProxy tool={}>", p.tool_name),
            Self::Type(n) | Self::Class(n) => write!(f, "<class '{n}'>"),
            Self::Module(n) => write!(f, "<module '{n}'>"),
            Self::Instance(inst) => write!(f, "<{} object>", inst.class_name),
            Self::ModuleFunction { name, .. } | Self::BuiltinName(name) => {
                write!(f, "<built-in function {name}>")
            }
            // chrono's `NaiveDate` Display is ISO 8601 (`2026-01-01`), matching
            // Python's `str(date)`.
            Self::Date(d) => write!(f, "{d}"),
            Self::ReMatch(m) => match m.groups.first().and_then(Option::as_ref) {
                Some(whole) => write!(
                    f,
                    "<re.Match object; span=({}, {}), match='{}'>",
                    whole.start, whole.end, whole.text
                ),
                None => write!(f, "<re.Match object>"),
            },
            // CPython: `<super: <class 'D'>, <C object>>`. Our model
            // carries only names; we mirror the shape for parity-of-repr
            // even though `super` values are usually consumed
            // immediately via `.method(...)` and rarely printed.
            Self::Super { defining_class, instance } => {
                write!(f, "<super: <class '{defining_class}'>, <{} object>>", instance.class_name)
            }
            // CPython: `2026-01-15 14:30:00` for naive datetime;
            // `2026-01-15 14:30:00+00:00` for aware.
            Self::DateTime { dt, tz_offset_secs } => {
                write!(f, "{}", dt.format("%Y-%m-%d %H:%M:%S"))?;
                if let Some(secs) = tz_offset_secs {
                    write_tz_offset(f, *secs)?;
                }
                Ok(())
            }
            // CPython: `14:30:00`.
            Self::Time(t) => write!(f, "{}", t.format("%H:%M:%S")),
            // CPython: `1 day, 3:04:05.000007` etc.
            Self::TimeDelta(micros) => write_timedelta(f, *micros),
            // CPython: `UTC` for offset 0; `UTC+05:00` otherwise.
            Self::TimeZone(secs) => {
                if *secs == 0 {
                    write!(f, "UTC")
                } else {
                    write!(f, "UTC")?;
                    write_tz_offset(f, *secs)
                }
            }
            // CPython: `<sha256 _hashlib.HASH object @ 0x...>`. We
            // simplify to a stable repr that surfaces the algorithm
            // and hex digest length — useful for debugging without
            // exposing process addresses.
            Self::HashDigest { algo, bytes } => {
                write!(f, "<{algo} HASH object, len={}>", bytes.len())
            }
            // CPython: `deque([1, 2, 3])` or `deque([1, 2], maxlen=3)`.
            Self::Deque { items, maxlen } => {
                write!(f, "deque([")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item.repr())?;
                }
                write!(f, "]")?;
                if let Some(n) = maxlen {
                    write!(f, ", maxlen={n}")?;
                }
                write!(f, ")")
            }
            // CPython: `Color.RED` for plain Enum members.
            Self::EnumMember { class_name, member_name, .. } => {
                write!(f, "{class_name}.{member_name}")
            }
            // CPython: `defaultdict(<factory>, {'a': 1, 'b': 2})`.
            Self::DefaultDict(data) => {
                write!(f, "defaultdict({}, {{", data.factory)?;
                for (i, (k, v)) in data.items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v.repr())?;
                }
                write!(f, "}})")
            }
            // CPython: empty Counter prints `Counter()`. Non-empty
            // prints `Counter({...})` with entries sorted by count
            // descending, insertion order as the tie-breaker
            // (CPython's `sorted(self, key=self.get, reverse=True)`).
            Self::Counter(map) => {
                if map.is_empty() {
                    return write!(f, "Counter()");
                }
                let mut entries: Vec<(&ValueKey, &Self)> = map.iter().collect();
                entries.sort_by(|a, b| {
                    let av = counter_value_as_i64(a.1);
                    let bv = counter_value_as_i64(b.1);
                    bv.cmp(&av)
                });
                write!(f, "Counter({{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v.repr())?;
                }
                write!(f, "}})")
            }
            // CPython `str(Decimal(...))` returns the exact digit string
            // (no exponent unless the input used one). BigDecimal's
            // Display matches that for our use; we strip its scientific-
            // notation tail when the value is finite and small enough.
            Self::Decimal(d) => format_decimal_str(f, d),
            // CPython `str(Fraction(n, d))` returns `n/d` (or just `n`
            // when d == 1). BigRational's Display already produces this
            // shape with a guaranteed positive denominator.
            Self::Fraction(f_val) => write!(f, "{f_val}"),
            // CPython: `<built-in method get of dict object at 0x...>`.
            // We drop the address (process-leaking) but keep the rest
            // so `print(d.get)` is readable. For Place-rooted receivers
            // we look up the type name lazily — `BoundMethodReceiver`
            // carries the place, not the value, so we render against
            // the method+root pair instead.
            Self::BoundMethod { receiver, method } => match receiver {
                BoundMethodReceiver::Snapshot(value) => {
                    write!(f, "<built-in method {method} of {} object>", value.python_type_name())
                }
                BoundMethodReceiver::Place { root, .. } => {
                    write!(f, "<built-in method {method} of {root}>")
                }
            },
            // CPython: `<method 'upper' of 'str' objects>`. The unbound
            // descriptor form — produced by `str.upper`, not `s.upper`.
            Self::BuiltinTypeMethod { type_name, method } => {
                write!(f, "<method '{method}' of '{type_name}' objects>")
            }
            // Bare-name references render as the canonical CPython
            // surface for that callable shape so a planner LLM sees
            // the same repr as a traceback frame.
            Self::ToolName(name) => write!(f, "<tool {name}>"),
            Self::ExceptionType(name) => write!(f, "<class '{name}'>"),
            Self::UnboundClassMethod { class, method } => {
                write!(f, "<bound method {class}.{method}>")
            }
            // CPython renders as `<generator object <name> at 0x...>`
            // — we don't track the source name or address so a stable
            // placeholder suffices for printing.
            Self::Lazy { .. } => write!(f, "<generator object>"),
            Self::Generator { .. } => write!(f, "<generator object>"),
            Self::BuiltinIter { kind, .. } => write!(f, "<{} object>", kind.type_name()),
            Self::Partial(data) => write!(f, "functools.partial({})", data.func),
            Self::OperatorGetter(g) => match &**g {
                OperatorGetter::ItemGetter(items) => {
                    let rendered: Vec<String> = items.iter().map(|v| v.repr()).collect();
                    write!(f, "operator.itemgetter({})", rendered.join(", "))
                }
                OperatorGetter::AttrGetter(attrs) => {
                    let rendered: Vec<String> =
                        attrs.iter().map(|parts| format!("'{}'", parts.join("."))).collect();
                    write!(f, "operator.attrgetter({})", rendered.join(", "))
                }
                OperatorGetter::MethodCaller { name, .. } => {
                    write!(f, "operator.methodcaller('{name}')")
                }
            },
            Self::LruCache(_) => write!(f, "<functools._lru_cache_wrapper>"),
            // CPython: `str(re.compile('a+b'))` == "re.compile('a+b')". The
            // pattern is rendered via its string repr, so backslashes and
            // quotes escape as in `re.compile('(\\d+)')`.
            Self::RePattern(p) => write!(f, "re.compile({})", python_str_repr(p)),
            // CPython: `repr(slice(1, 5, 2))` == "slice(1, 5, 2)".
            Self::Slice(s) => write!(f, "slice({}, {}, {})", s.start, s.stop, s.step),
            // CPython renders `<memory at 0x...>` with a real address; we omit
            // the (unstable) pointer.
            Self::MemoryView(_) => write!(f, "<memory>"),
        }
    }
}

/// CPython-shape `str(Decimal)`: preserve the input scale exactly
/// (`Decimal("5")` is "5", `Decimal("5.0")` is "5.0"). BigDecimal's
/// `to_plain_string` emits the canonical positional form without
/// scientific notation, matching CPython's `str(Decimal)` output for
/// the common ranges. CPython's scientific notation thresholds for
/// extreme magnitudes are tracked by `gap-decimal-scientific-formatting`.
fn format_decimal_str(f: &mut fmt::Formatter<'_>, d: &bigdecimal::BigDecimal) -> fmt::Result {
    write!(f, "{}", d.to_plain_string())
}

/// Write a Python bytes literal (`b'...'`) with CPython's quote selection and
/// escaping. Shared by `bytes` and `bytearray` rendering.
fn write_bytes_literal(f: &mut fmt::Formatter<'_>, b: &[u8]) -> fmt::Result {
    let has_single = b.contains(&b'\'');
    let has_double = b.contains(&b'"');
    let quote = if has_single && !has_double { b'"' } else { b'\'' };
    write!(f, "b{}", quote as char)?;
    for &byte in b {
        match byte {
            b'\\' => write!(f, "\\\\")?,
            b'\n' => write!(f, "\\n")?,
            b'\r' => write!(f, "\\r")?,
            b'\t' => write!(f, "\\t")?,
            b if b == quote => write!(f, "\\{}", b as char)?,
            0x20..=0x7E => write!(f, "{}", byte as char)?,
            _ => write!(f, "\\x{byte:02x}")?,
        }
    }
    write!(f, "{}", quote as char)
}

/// Python `repr()` of a string: CPython's quote selection plus escaping.
/// Single quotes are preferred, switching to double only when the string
/// contains a single quote but no double quote. Backslash, the active quote,
/// and the C0/DEL control characters are escaped; printable Unicode is kept
/// verbatim (matching `str.isprintable()` for the common ranges).
#[must_use]
pub fn python_str_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (c as u32) == 0x7f => {
                use std::fmt::Write as _;
                // C0 controls and DEL render as \xNN, as CPython does.
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

impl Value {
    /// The bare class name from a possibly module-qualified type name.
    /// `"statistics.StatisticsError"` → `"StatisticsError"`; a name with no
    /// dot passes through. This is CPython's `type.__name__` (never the
    /// module-qualified form, which only appears in tracebacks).
    #[must_use]
    pub fn short_type_name(name: &str) -> &str {
        name.rsplit('.').next().unwrap_or(name)
    }

    /// Python `repr()` — strings are quoted, other types match `str()`.
    #[must_use]
    pub fn repr(&self) -> String {
        match self {
            Self::String(s) => python_str_repr(s),
            // CPython: `repr(date(2026, 1, 1))` == "datetime.date(2026, 1, 1)".
            Self::Date(d) => {
                use chrono::Datelike;
                format!("datetime.date({}, {}, {})", d.year(), d.month(), d.day())
            }
            // CPython: repr(ValueError('boom')) == "ValueError('boom')".
            // Single-quoted message, type prefix. Display is str() form
            // (just the message); repr surfaces the typed shape. The type
            // prefix is the bare class name — CPython's exception repr never
            // qualifies with the module, even for `statistics.StatisticsError`.
            Self::Exception(e) => {
                // CPython's `BaseException.__repr__`:
                // `Type(repr(a1), repr(a2), ...)` over `self.args`.
                let name = Self::short_type_name(&e.type_name);
                let inner = e.args.iter().map(Self::repr).collect::<Vec<_>>().join(", ");
                format!("{name}({inner})")
            }
            other => format!("{other}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Accessor methods for safe value extraction
// ---------------------------------------------------------------------------

impl Value {
    /// Get as string reference if this is a `Value::String`.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Get as i64 if this is a `Value::Int`.
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            Self::BigInt(b) => i64::try_from(b.as_ref()).ok(),
            _ => None,
        }
    }

    /// Get as f64 if this is a `Value::Float` or `Value::Int`.
    ///
    /// Int-to-float conversion can lose precision for values beyond
    /// 2^53; this matches Python's `float(int)` semantics.
    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            #[expect(
                clippy::cast_precision_loss,
                reason = "matches Python's `float(int)` semantic: the standard \
                          library is lossy for ints beyond 2^53 and we faithfully \
                          reproduce that"
            )]
            Self::Int(i) => Some(*i as f64),
            Self::BigInt(b) => {
                // Lossy for huge ints — matches CPython float(int).
                use num_traits::ToPrimitive as _;
                b.to_f64()
            }
            Self::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Self::Decimal(d) => {
                use num_traits::ToPrimitive as _;
                d.to_f64()
            }
            Self::Fraction(fr) => {
                use num_traits::ToPrimitive as _;
                fr.to_f64()
            }
            _ => None,
        }
    }

    /// Get as bool if this is a `Value::Bool`.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Get a locked guard over the inner `Vec<Value>` if this is a
    /// `Value::List`. The guard derefs to `Vec<Value>`, so callers can
    /// `.len()`, `.iter()`, and index just like a slice — but the lock
    /// is held for the guard's lifetime, so don't keep it across other
    /// container operations.
    #[must_use]
    pub fn as_list(&self) -> Option<parking_lot::MutexGuard<'_, Vec<Self>>> {
        match self {
            Self::List(items) => Some(items.lock()),
            _ => None,
        }
    }

    /// Get as dict reference if this is a `Value::Dict`.
    #[must_use]
    pub const fn as_dict(&self) -> Option<&IndexMap<ValueKey, Self>> {
        match self {
            Self::Dict(map) => Some(map),
            _ => None,
        }
    }

    /// Consume and extract the inner String, returning Err(self) if not a string.
    ///
    /// # Errors
    ///
    /// Returns `Err(self)` when the value isn't a `Value::String`, letting
    /// the caller recover the original value without a clone.
    pub fn try_into_string(self) -> Result<String, Self> {
        match self {
            Self::String(s) => Ok(s.to_string()),
            other => Err(other),
        }
    }

    /// Consume and extract the inner Vec, returning Err(self) if not a list.
    ///
    /// # Errors
    ///
    /// Returns `Err(self)` when the value isn't a `Value::List`. When the
    /// `SharedList` has exactly one strong reference, the inner Vec moves
    /// out without an allocation; when aliased, the contents are cloned.
    pub fn try_into_list(self) -> Result<Vec<Self>, Self> {
        match self {
            Self::List(items) => Ok(match Arc::try_unwrap(items) {
                Ok(mutex) => mutex.into_inner(),
                Err(shared) => shared.lock().clone(),
            }),
            other => Err(other),
        }
    }

    /// Consume and extract the inner `IndexMap`, returning Err(self) if not a dict.
    ///
    /// # Errors
    ///
    /// Returns `Err(self)` when the value isn't a `Value::Dict`.
    pub fn try_into_dict(self) -> Result<IndexMap<ValueKey, Self>, Self> {
        match self {
            Self::Dict(map) => Ok(map),
            other => Err(other),
        }
    }
}

// ---------------------------------------------------------------------------
// From impls for ergonomic Value construction
// ---------------------------------------------------------------------------

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Self::Int(i64::from(v))
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::String(v.into())
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::String(v.into())
    }
}
impl From<Vec<Self>> for Value {
    fn from(v: Vec<Self>) -> Self {
        Self::List(shared_list(v))
    }
}
impl From<IndexMap<ValueKey, Self>> for Value {
    fn from(v: IndexMap<ValueKey, Self>) -> Self {
        Self::Dict(v)
    }
}
impl<T: Into<Self>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        v.map_or(Self::None, Into::into)
    }
}

// ---------------------------------------------------------------------------
// JSON conversion
// ---------------------------------------------------------------------------

impl Value {
    /// Convert a `serde_json::Value` into an interpreter `Value`.
    ///
    /// Maps JSON types naturally:
    /// - `null` → `None`
    /// - `bool` → `Bool`
    /// - integer numbers → `Int`
    /// - fractional numbers → `Float`
    /// - `string` → `String`
    /// - `array` → `List`
    /// - `object` → `Dict` (string keys)
    pub fn from_json(json: serde_json::Value) -> Self {
        match json {
            serde_json::Value::Null => Self::None,
            serde_json::Value::Bool(b) => Self::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Self::Int(i)
                } else {
                    // Beyond i64. With serde_json's `arbitrary_precision`, the
                    // raw literal is preserved: an integer literal (no `.`/`e`)
                    // promotes to an exact BigInt rather than a lossy float.
                    let raw = n.to_string();
                    if raw.contains(['.', 'e', 'E']) {
                        n.as_f64().map(Self::Float).unwrap_or(Self::None)
                    } else {
                        raw.parse::<num_bigint::BigInt>().map_or_else(
                            |_| n.as_f64().map(Self::Float).unwrap_or(Self::None),
                            int_from_bigint,
                        )
                    }
                }
            }
            serde_json::Value::String(s) => Self::String(s.into()),
            serde_json::Value::Array(arr) => {
                Self::List(shared_list(arr.into_iter().map(Self::from_json).collect()))
            }
            serde_json::Value::Object(obj) => {
                let mut map = IndexMap::new();
                for (k, v) in obj {
                    map.insert(ValueKey::String(k.into()), Self::from_json(v));
                }
                Self::Dict(map)
            }
        }
    }

    /// Convert an interpreter `Value` to a `serde_json::Value`.
    ///
    /// Data-bearing types are encoded (dict-likes as objects, list-likes as
    /// arrays, `Decimal` as an exact number, dates as ISO strings, `timedelta`
    /// as seconds, an int/str enum member as its value). Types with no JSON
    /// form — callables, ranges, exceptions, proxies, singletons, `complex` —
    /// raise `TypeError`, rather than silently collapsing to `null` (the old
    /// `_ => Null` lost `Decimal`/date/`Counter` data without warning).
    ///
    /// # Errors
    /// Returns `TypeError("Object of type <t> is not JSON serializable")` for a
    /// value (or nested element) with no JSON representation.
    pub fn to_json(&self) -> Result<serde_json::Value, crate::error::InterpreterError> {
        use serde_json::Value as J;
        let array = |items: &[Self]| -> Result<J, crate::error::InterpreterError> {
            Ok(J::Array(items.iter().map(Self::to_json).collect::<Result<_, _>>()?))
        };
        Ok(match self {
            Self::None => J::Null,
            Self::Bool(b) => J::Bool(*b),
            Self::Int(i) => serde_json::json!(*i),
            // JSON numbers are f64; huge ints stringify to preserve digits.
            Self::BigInt(i) => J::String(i.to_string()),
            Self::Float(f) => serde_json::json!(*f),
            Self::String(s) => J::String(s.to_string()),
            Self::Bytes(b) => serde_json::json!(b),
            // List / Tuple / Set / Deque all project to a JSON array.
            Self::List(items) => array(&items.lock())?,
            Self::Tuple(items) | Self::Set(items) | Self::Frozenset(items) => array(items)?,
            Self::Deque { items, .. } => {
                J::Array(items.iter().map(Self::to_json).collect::<Result<_, _>>()?)
            }
            // Dict / Counter / defaultdict project to a JSON object.
            Self::Dict(map) => json_object(map.iter())?,
            Self::Counter(map) => json_object(map.iter())?,
            Self::DefaultDict(data) => json_object(data.items.iter())?,
            // Decimal keeps its exact digits via arbitrary_precision; Fraction
            // has no JSON form, so its float value is used.
            Self::Decimal(d) => {
                serde_json::from_str(&d.to_string()).unwrap_or_else(|_| J::String(d.to_string()))
            }
            Self::Fraction(fr) => {
                use num_traits::ToPrimitive as _;
                serde_json::json!(fr.to_f64().unwrap_or(f64::NAN))
            }
            // An int/str enum member serialises as its underlying value.
            Self::EnumMember { value, .. } => value.to_json()?,
            // Dates/times as ISO strings; timedelta as fractional seconds.
            Self::Date(d) => J::String(d.format("%Y-%m-%d").to_string()),
            Self::DateTime { dt, .. } => J::String(dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
            Self::Time(t) => J::String(t.format("%H:%M:%S").to_string()),
            #[expect(clippy::cast_precision_loss, reason = "seconds as f64 is the host JSON form")]
            Self::TimeDelta(us) => serde_json::json!(*us as f64 / 1_000_000.0),
            other => {
                return Err(crate::error::InterpreterError::TypeError(format!(
                    "Object of type {} is not JSON serializable",
                    other.type_name()
                )));
            }
        })
    }
}

/// Project a `ValueKey`-keyed map into a JSON object, stringifying non-string
/// keys the way CPython's json encoder coerces scalar keys.
fn json_object<'a, I>(entries: I) -> Result<serde_json::Value, crate::error::InterpreterError>
where
    I: Iterator<Item = (&'a ValueKey, &'a Value)>,
{
    let mut obj = serde_json::Map::new();
    for (k, v) in entries {
        let key = match k {
            ValueKey::String(s) => s.to_string(),
            other => format!("{other}"),
        };
        obj.insert(key, v.to_json()?);
    }
    Ok(serde_json::Value::Object(obj))
}

impl Value {
    /// Derive the hashable dict/set key for this value.
    ///
    /// The public inverse of [`ValueKey::to_value`]. Applies the same key
    /// coercion the evaluator does — notably, an integral float folds into
    /// [`ValueKey::Int`], so `{2: x}[2.0]` hits one slot, matching CPython's
    /// `hash(2.0) == hash(2)`.
    ///
    /// Exposed because any host that builds a [`Value::Dict`] from outside the
    /// crate — the language bindings, chiefly — needs to construct keys, and a
    /// second hand-rolled implementation of the folding rules would silently
    /// diverge from the evaluator's: a dict holding two equal-but-distinct keys
    /// corrupts `in` / `len` / lookup.
    ///
    /// # Errors
    ///
    /// Returns [`InterpreterError::TypeError`] (`unhashable type: '...'`) for
    /// values Python cannot use as a key — `list`, `dict`, `set`, and the
    /// interpreter's internal variants.
    pub fn to_key(&self) -> Result<ValueKey, crate::error::InterpreterError> {
        match crate::eval::literals::value_to_key(self) {
            Ok(key) => Ok(key),
            Err(crate::error::EvalError::Interpreter(e)) => Err(e),
            // `value_to_key` only ever fails as Interpreter(TypeError): it
            // raises no Python exception and emits no control-flow signal.
            // The arm exists because EvalError is #[non_exhaustive] to us.
            Err(_) => Err(crate::error::InterpreterError::TypeError(format!(
                "unhashable type: '{}'",
                self.type_name()
            ))),
        }
    }
}

impl ValueKey {
    /// Reconstruct the `Value` this key was derived from.
    ///
    /// Inverse of dict-key coercion for the variants that
    /// round-trip (an integral float folded to `Int` comes back as `Int`, by
    /// design — see `value_to_key`). Centralised so adding a `ValueKey` variant
    /// has exactly one conversion site to update, not the several hand-rolled
    /// matches that previously drifted across the evaluator.
    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            Self::None => Value::None,
            Self::Ellipsis => Value::Ellipsis,
            Self::Bool(b) => Value::Bool(*b),
            Self::Int(i) => Value::Int(*i),
            Self::BigInt(i) => crate::value::int_from_bigint(i.clone()),
            Self::Float(bits) => Value::Float(f64::from_bits(*bits)),
            Self::Complex(re, im) => Value::Complex(Box::new(num_complex::Complex64::new(
                f64::from_bits(*re),
                f64::from_bits(*im),
            ))),
            Self::String(s) => Value::String(s.clone()),
            Self::Tuple(items) => Value::Tuple(items.iter().map(Self::to_value).collect()),
            Self::Frozenset(items) => Value::Frozenset(items.iter().map(Self::to_value).collect()),
            Self::Instance { value, .. } => (**value).clone(),
        }
    }
}

impl fmt::Display for ValueKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Ellipsis => write!(f, "Ellipsis"),
            Self::Bool(true) => write!(f, "True"),
            Self::Bool(false) => write!(f, "False"),
            Self::Int(i) => write!(f, "{i}"),
            Self::BigInt(i) => write!(f, "{i}"),
            // Integral floats never reach this variant (folded to Int); the
            // shared formatter still handles them for parity if one is built
            // directly.
            Self::Float(bits) => write_python_float(f, f64::from_bits(*bits)),
            Self::Complex(re, im) => write!(
                f,
                "{}",
                format_complex(&num_complex::Complex64::new(
                    f64::from_bits(*re),
                    f64::from_bits(*im)
                ))
            ),
            Self::String(s) => write!(f, "'{s}'"),
            Self::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                if items.len() == 1 {
                    write!(f, ",")?;
                }
                write!(f, ")")
            }
            Self::Frozenset(items) => {
                if items.is_empty() {
                    return write!(f, "frozenset()");
                }
                write!(f, "frozenset({{")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "}})")
            }
            Self::Instance { value, .. } => write!(f, "{value}"),
        }
    }
}
