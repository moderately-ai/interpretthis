// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Value` <-> JavaScript conversion.
//!
//! Native, not JSON. `Value::to_json` exists and is tempting, but it stringifies
//! `BigInt`, turns `Bytes` into an array of ints, collapses `List`/`Tuple`/`Set`
//! into one array, and quietly maps `Decimal`, `Date`, and every user object to
//! `null`. JavaScript has real analogues for most of that — `BigInt`, `Buffer`,
//! `Set`, `Map`, `Date` — so we use them.
//!
//! # The mapping, including where it is lossy
//!
//! Python is richer than JavaScript here, and a few edges cannot round-trip.
//! Rather than guess, the asymmetries are fixed and documented:
//!
//! | `Value`            | to JS                    | from JS         |
//! |--------------------|--------------------------|-----------------|
//! | `None`             | `null`                   | `null`/`undefined` |
//! | `Bool`             | `boolean`                | `boolean`       |
//! | `Int`              | `number`, or `BigInt` when it exceeds 2^53-1 | `number` (integral) |
//! | `BigInt`           | `BigInt`                 | `BigInt`        |
//! | `Float`            | `number`                 | `number` (non-integral) |
//! | `String`           | `string`                 | `string`        |
//! | `Bytes`            | `Buffer`                 | `Buffer`/`Uint8Array` |
//! | `List`             | `Array`                  | `Array`         |
//! | `Tuple`            | **frozen** `Array`       | — (an array is always a `list`) |
//! | `Set`              | `Set`                    | `Set`           |
//! | `Dict`             | plain object, or `Map` when a key is not a string | object / `Map` |
//! | `DateTime`/`Date`  | `Date`                   | `Date`          |
//!
//! Two consequences worth stating plainly, because they surprise people:
//!
//! - **A JS array always becomes a Python `list`, never a `tuple`.** JavaScript
//!   has no tuple to distinguish, so a `Value::Tuple` handed to JS comes back as
//!   a `list`. It is frozen on the way out to signal that it *was* immutable.
//! - **`Int` above `Number.MAX_SAFE_INTEGER` becomes a `BigInt`,** because a JS
//!   `number` would silently lose precision. So the JS type of a sandbox integer
//!   depends on its magnitude. Losing digits silently is worse.
//!
//! A `Dict` becomes a plain object in the common all-string-keys case, and a
//! `Map` otherwise — an object would stringify `{1: "a"}` into `{"1": "a"}` and
//! lose the key's type.
//!
//! # Copy, not alias
//!
//! Crossing this boundary copies. A JS array passed in as a variable is not
//! aliased by sandboxed code; mutations the script makes are not visible to the
//! caller's object. Read results back with `getVariable`. Aliasing would hand
//! sandboxed code a live handle on host memory, which is what this interpreter
//! exists to prevent.

use chrono::{DateTime, NaiveDateTime, Utc};
use indexmap::IndexMap;
use interpretthis::{Value, ValueKey, shared_list};
use napi::{
    Env, Error, JsValue as _, Result, Status, ValueType,
    bindgen_prelude::{
        Array, BigInt, Buffer, FromNapiValue, Function, JsObjectValue as _, Null, Object,
        ToNapiValue, TypeName, Unknown, ValidateNapiValue, sys,
    },
};

/// Largest integer JavaScript's `number` represents exactly: 2^53 - 1.
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

fn type_error(message: String) -> Error {
    Error::new(Status::InvalidArg, message)
}

/// A [`Value`] as it crosses the JavaScript boundary.
///
/// A newtype because the conversion traits are foreign and `Value` is foreign.
/// Owned by the time it reaches Rust, so it is `Send` — which matters: the
/// conversion runs on the JS thread inside a threadsafe-function callback, and
/// the result then travels to a tokio worker.
pub struct SandboxValue(pub Value);

impl TypeName for SandboxValue {
    fn type_name() -> &'static str {
        "SandboxValue"
    }

    fn value_type() -> ValueType {
        // No single JS type: a sandbox value may arrive as any of them.
        ValueType::Unknown
    }
}

impl ValidateNapiValue for SandboxValue {
    unsafe fn validate(_env: sys::napi_env, _napi_val: sys::napi_value) -> Result<sys::napi_value> {
        // Accept anything. The default implementation compares against
        // `value_type()`, which for us is `Unknown` and would therefore reject
        // every real value.
        //
        // This is also what makes `Either<Promise<SandboxValue>, SandboxValue>`
        // work as the tool return type: `Promise` validates first (is it a
        // thenable?), and anything that is not falls through to here. The real
        // type check happens in `from_napi_value`, which produces a precise
        // error naming the offending type.
        Ok(std::ptr::null_mut())
    }
}

// ---------------------------------------------------------------------------
// Value -> JavaScript
// ---------------------------------------------------------------------------

impl ToNapiValue for SandboxValue {
    unsafe fn to_napi_value(raw_env: sys::napi_env, val: Self) -> Result<sys::napi_value> {
        let env = Env::from_raw(raw_env);
        let unknown = value_to_js(&env, &val.0)?;
        unsafe { Unknown::to_napi_value(raw_env, unknown) }
    }
}

/// Convert an interpreter [`Value`] into a JavaScript value.
///
/// # Errors
///
/// Returns an error for variants with no JavaScript analogue (functions,
/// classes, instances, generators, pending tool proxies, ...). Never `null`:
/// silently handing back `null` for a value that *is* something turns a boundary
/// error into a wrong answer somewhere downstream.
pub fn value_to_js<'env>(env: &'env Env, value: &Value) -> Result<Unknown<'env>> {
    match value {
        Value::None | Value::NotImplemented => Null.into_unknown(env),
        Value::Bool(b) => b.into_unknown(env),

        // Above 2^53-1 a JS `number` loses digits. Promote rather than corrupt.
        // `unsigned_abs` avoids the `i64::MIN.abs()` overflow (which is well
        // outside the safe range and promotes to BigInt anyway).
        Value::Int(i) => {
            if i.unsigned_abs() <= MAX_SAFE_INTEGER as u64 {
                (*i as f64).into_unknown(env)
            } else {
                BigInt::from(*i).into_unknown(env)
            }
        }
        Value::BigInt(i) => bigint_to_js(env, i),
        Value::Float(f) => f.into_unknown(env),
        Value::String(s) => s.as_str().into_unknown(env),
        Value::Bytes(b) => Buffer::from(b.clone()).into_unknown(env),

        Value::List(items) => {
            // The lock is released before any JS object is built, so a tool
            // callback cannot deadlock against this guard.
            let snapshot: Vec<Value> = items.lock().clone();
            array_to_js(env, &snapshot, false)
        }
        // Frozen to signal that this was a tuple. It still comes back as a list:
        // JavaScript has no tuple to round-trip through.
        Value::Tuple(items) => array_to_js(env, items, true),

        Value::Set(items) => {
            let array = array_to_js(env, items, false)?;
            construct_global(env, "Set", array)
        }

        Value::Dict(map) => dict_to_js(env, map),

        // A Python `range` is a lazy sequence; JS has no counterpart, so it
        // materialises as an array. The alternative — an opaque handle — would
        // be useless to a JS caller.
        Value::Range { start, stop, step } => {
            let items: Vec<Value> = range_items(*start, *stop, *step)?;
            array_to_js(env, &items, false)
        }

        Value::DateTime { dt, tz_offset_secs } => {
            let utc: DateTime<Utc> = tz_offset_secs.map_or_else(
                || DateTime::<Utc>::from_naive_utc_and_offset(*dt, Utc),
                |secs| {
                    DateTime::<Utc>::from_naive_utc_and_offset(
                        *dt - chrono::TimeDelta::seconds(i64::from(secs)),
                        Utc,
                    )
                },
            );
            utc.into_unknown(env)
        }
        Value::Date(d) => {
            let dt: NaiveDateTime = d.and_hms_opt(0, 0, 0).ok_or_else(|| {
                type_error("date is not representable as a JavaScript Date".to_string())
            })?;
            DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).into_unknown(env)
        }

        // Decimal and Fraction have no lossless JS counterpart — a `number`
        // would defeat the entire point of an exact decimal. They cross as their
        // exact string form, which is documented and lossy in one direction.
        Value::Decimal(d) => d.to_string().into_unknown(env),
        Value::Fraction(f) => f.to_string().into_unknown(env),

        // Value is #[non_exhaustive]; the rest is interpreter-internal.
        other => Err(type_error(format!(
            "interpretthis: cannot convert sandbox value of type '{}' to a JavaScript value; \
             it exists only inside the interpreter",
            other.type_name()
        ))),
    }
}

fn bigint_to_js<'env>(env: &'env Env, value: &num_bigint::BigInt) -> Result<Unknown<'env>> {
    let (sign, words) = value.to_u64_digits();
    BigInt { sign_bit: sign == num_bigint::Sign::Minus, words }.into_unknown(env)
}

/// Cap on how many elements a `range` may materialise into a JS array. A larger
/// range would exhaust memory; the interpreter keeps ranges lazy, but the JS
/// boundary has no lazy sequence, so an oversized range is an error, not an OOM.
const MAX_RANGE_MATERIALIZE: i128 = 100_000_000;

fn range_items(start: i64, stop: i64, step: i64) -> Result<Vec<Value>> {
    if step == 0 {
        return Ok(Vec::new());
    }
    // Count in i128 so `stop - start` (up to ~2^64) cannot overflow, and reject
    // an oversized range before allocating.
    let (s, e, st) = (i128::from(start), i128::from(stop), i128::from(step));
    let span = e - s;
    let count: i128 = if (st > 0 && span > 0) || (st < 0 && span < 0) {
        span / st + i128::from(span % st != 0)
    } else {
        0
    };
    if count > MAX_RANGE_MATERIALIZE {
        return Err(Error::new(
            Status::GenericFailure,
            format!("range with {count} elements is too large to convert to a JS array"),
        ));
    }
    let mut items = Vec::with_capacity(count.max(0) as usize);
    let mut current = start;
    // `checked_add` guards the `i64::MAX`/`i64::MIN` edge (a step past the bound
    // would otherwise wrap and loop forever).
    while (step > 0 && current < stop) || (step < 0 && current > stop) {
        items.push(Value::Int(current));
        match current.checked_add(step) {
            Some(next) => current = next,
            None => break,
        }
    }
    Ok(items)
}

fn array_to_js<'env>(env: &'env Env, items: &[Value], freeze: bool) -> Result<Unknown<'env>> {
    // `Vec<T: ToNapiValue>` already converts to a JS array, so there is no need
    // to reach for the (private) Array constructor.
    let converted: Vec<SandboxValue> = items.iter().cloned().map(SandboxValue).collect();
    let unknown = converted.into_unknown(env)?;

    if freeze {
        return freeze_value(env, unknown);
    }
    Ok(unknown)
}

fn dict_to_js<'env>(env: &'env Env, map: &IndexMap<ValueKey, Value>) -> Result<Unknown<'env>> {
    // A plain object cannot hold a non-string key without stringifying it —
    // `{1: "a"}` would become `{"1": "a"}` and lose the key's type. So the
    // common case gets an ergonomic object, and anything else gets a Map.
    let all_string_keys = map.keys().all(|k| matches!(k, ValueKey::String(_)));

    if all_string_keys {
        let mut object = Object::new(env)?;
        for (key, value) in map {
            let ValueKey::String(name) = key else {
                unreachable!("every key was checked to be a string above")
            };
            object.set(name.as_str(), SandboxValue(value.clone()))?;
        }
        return object.into_unknown(env);
    }

    // `new Map([[k, v], ...])`.
    let entries: Vec<Vec<SandboxValue>> = map
        .iter()
        .map(|(key, value)| vec![SandboxValue(key.to_value()), SandboxValue(value.clone())])
        .collect();
    let entries = entries.into_unknown(env)?;
    construct_global(env, "Map", entries)
}

/// `new global[name](arg)` — used for `Set` and `Map`, which napi has no
/// constructor for.
fn construct_global<'env>(env: &'env Env, name: &str, arg: Unknown<'env>) -> Result<Unknown<'env>> {
    let global = env.get_global()?;
    let constructor: Function<Unknown, Unknown> = global.get_named_property(name)?;
    constructor.new_instance(arg)?.into_unknown(env)
}

fn freeze_value<'env>(env: &'env Env, value: Unknown<'env>) -> Result<Unknown<'env>> {
    let global = env.get_global()?;
    // `Object` on globalThis is a constructor *function*, not an object — asking
    // for it as an `Object` fails the type check.
    let object_ctor: Function<Unknown, Unknown> = global.get_named_property("Object")?;
    let freeze: Function<Unknown, Unknown> =
        object_ctor.coerce_to_object()?.get_named_property("freeze")?;
    freeze.call(value)
}

// ---------------------------------------------------------------------------
// JavaScript -> Value
// ---------------------------------------------------------------------------

impl FromNapiValue for SandboxValue {
    unsafe fn from_napi_value(raw_env: sys::napi_env, napi_val: sys::napi_value) -> Result<Self> {
        let env = Env::from_raw(raw_env);
        let unknown = unsafe { Unknown::from_napi_value(raw_env, napi_val)? };
        Ok(Self(js_to_value(&env, unknown)?))
    }
}

/// Convert a JavaScript value into an interpreter [`Value`].
///
/// # Errors
///
/// Returns an error for JavaScript values with no sandbox analogue (functions,
/// symbols, class instances), and for unhashable dict keys.
pub fn js_to_value(env: &Env, value: Unknown<'_>) -> Result<Value> {
    match value.get_type()? {
        ValueType::Null | ValueType::Undefined => Ok(Value::None),
        ValueType::Boolean => Ok(Value::Bool(value.coerce_to_bool()?)),
        ValueType::String => {
            Ok(Value::String(value.coerce_to_string()?.into_utf8()?.as_str()?.into()))
        }
        ValueType::BigInt => {
            let bigint: BigInt = BigInt::from_unknown(value)?;
            Ok(js_bigint_to_value(&bigint))
        }
        ValueType::Number => {
            let n: f64 = f64::from_unknown(value)?;
            // An integral JS number is a Python int; anything else is a float.
            // Without this, `1` would arrive as `1.0` and `x // 2` would behave
            // like float division to anyone reading the script.
            if n.fract() == 0.0 && n.abs() <= MAX_SAFE_INTEGER as f64 {
                Ok(Value::Int(n as i64))
            } else {
                Ok(Value::Float(n))
            }
        }
        ValueType::Object => object_to_value(env, value),
        other => Err(type_error(format!(
            "interpretthis: cannot pass a JavaScript {other} into the sandbox; supported types \
             are null, boolean, number, bigint, string, Buffer, Array, Set, Map, Date and plain \
             objects"
        ))),
    }
}

fn js_bigint_to_value(bigint: &BigInt) -> Value {
    let sign = if bigint.sign_bit { num_bigint::Sign::Minus } else { num_bigint::Sign::Plus };
    let big = num_bigint::BigInt::from_slice(
        sign,
        &bigint.words.iter().flat_map(|w| [*w as u32, (*w >> 32) as u32]).collect::<Vec<u32>>(),
    );
    // Values that fit i64 stay on the interpreter's fast Int path.
    interpretthis::value::int_from_bigint(big)
}

fn object_to_value(env: &Env, value: Unknown<'_>) -> Result<Value> {
    // Node's `is_buffer` returns true for *any* typed array, so both are gated
    // on the constructor: a Node `Buffer` / `Uint8Array` maps to bytes, but
    // another typed array (Float64Array, Int32Array, …) carries element
    // *values*, not raw bytes — returning the raw memory (e.g. 24 bytes for
    // `Float64Array([1,2,3])`) is silent corruption, so reject it.
    if value.is_buffer()? || value.is_typedarray()? {
        let object = value.coerce_to_object()?;
        match constructor_name(&object)?.as_deref() {
            Some("Buffer" | "Uint8Array" | "Uint8ClampedArray") => {
                let buffer: Buffer = Buffer::from_unknown(value)?;
                return Ok(Value::Bytes(buffer.to_vec()));
            }
            other => {
                return Err(type_error(format!(
                    "interpretthis: {} is not supported; convert it to a plain Array (numbers) \
                     or a Uint8Array (bytes) first",
                    other.unwrap_or("this typed array")
                )));
            }
        }
    }
    if value.is_date()? {
        let dt: DateTime<Utc> = DateTime::<Utc>::from_unknown(value)?;
        return Ok(Value::DateTime { dt: dt.naive_utc(), tz_offset_secs: Some(0) });
    }
    if value.is_array()? {
        let array = Array::from_unknown(value)?;
        let mut items = Vec::with_capacity(array.len() as usize);
        for index in 0..array.len() {
            let element: Unknown = array.get(index)?.ok_or_else(|| {
                type_error("a sparse array cannot be passed into the sandbox".to_string())
            })?;
            items.push(js_to_value(env, element)?);
        }
        // Always a list. JS has no tuple, so there is nothing to distinguish.
        return Ok(Value::List(shared_list(items)));
    }

    match global_class_of(env, &value)? {
        Some(class) if class == "Set" => set_to_value(env, &value),
        Some(class) if class == "Map" => map_to_value(env, &value),
        _ => plain_object_to_value(env, value),
    }
}

/// Name of the JS builtin this object is an instance of, for the handful we care
/// about. Checked by `instanceof` against the global, not by duck-typing.
fn global_class_of(env: &Env, value: &Unknown<'_>) -> Result<Option<String>> {
    let global = env.get_global()?;
    for name in ["Set", "Map"] {
        // Same as above: these are constructor functions.
        let constructor: Function<Unknown, Unknown> = global.get_named_property(name)?;
        if value.instanceof(constructor)? {
            return Ok(Some(name.to_string()));
        }
    }
    Ok(None)
}

fn set_to_value(env: &Env, value: &Unknown<'_>) -> Result<Value> {
    let object = value.coerce_to_object()?;
    let values: napi::bindgen_prelude::Function<(), Unknown> =
        object.get_named_property("values")?;
    let iterator = values.apply(object.to_unknown(), ())?;
    let items = drain_iterator(env, iterator)?;
    // A JS Set can hold an array/object; Python set elements must be hashable,
    // so validate each element (discarding the key) before building the set.
    for item in &items {
        to_key(item)?;
    }
    Ok(Value::Set(items))
}

fn map_to_value(env: &Env, value: &Unknown<'_>) -> Result<Value> {
    let object = value.coerce_to_object()?;
    let entries: napi::bindgen_prelude::Function<(), Unknown> =
        object.get_named_property("entries")?;
    let iterator = entries.apply(object.to_unknown(), ())?;
    let pairs = drain_iterator(env, iterator)?;

    let mut map: IndexMap<ValueKey, Value> = IndexMap::new();
    for pair in pairs {
        let Value::List(items) = pair else {
            return Err(type_error("malformed Map entry".to_string()));
        };
        let items = items.lock();
        let (Some(key), Some(val)) = (items.first(), items.get(1)) else {
            return Err(type_error("malformed Map entry".to_string()));
        };
        map.insert(to_key(key)?, val.clone());
    }
    Ok(Value::Dict(map))
}

/// Pull every item out of a JS iterator (`Set.values()`, `Map.entries()`).
fn drain_iterator(env: &Env, iterator: Unknown<'_>) -> Result<Vec<Value>> {
    let object = iterator.coerce_to_object()?;
    let next: napi::bindgen_prelude::Function<(), Object> = object.get_named_property("next")?;

    let mut items = Vec::new();
    loop {
        let step = next.apply(object.to_unknown(), ())?;
        let done: bool = step.get_named_property("done")?;
        if done {
            break;
        }
        let value: Unknown = step.get_named_property("value")?;
        items.push(js_to_value(env, value)?);
    }
    Ok(items)
}

/// The name of an object's constructor (`obj.constructor.name`), for the plain
/// vs class-instance and typed-array distinctions. `None` for a null-prototype
/// object or a missing/anonymous constructor.
fn constructor_name(object: &Object) -> Result<Option<String>> {
    let ctor: Unknown = object.get_named_property("constructor")?;
    if ctor.get_type()? != ValueType::Function {
        return Ok(None);
    }
    let ctor_obj = ctor.coerce_to_object()?;
    let name: Unknown = ctor_obj.get_named_property("name")?;
    if name.get_type()? == ValueType::String {
        Ok(Some(name.coerce_to_string()?.into_utf8()?.as_str()?.to_string()))
    } else {
        Ok(None)
    }
}

fn plain_object_to_value(env: &Env, value: Unknown<'_>) -> Result<Value> {
    let object = value.coerce_to_object()?;
    // Only a plain object (or a null-prototype object) becomes a dict. A class
    // instance — an Error, a Date subclass, a user class — must not silently
    // collapse to `{}` (its data lives in non-enumerable/prototype properties
    // that `Object.keys` never sees).
    match constructor_name(&object)?.as_deref() {
        None | Some("Object") => {}
        Some(other) => {
            return Err(type_error(format!(
                "interpretthis: cannot pass a JavaScript {other} instance into the sandbox; \
                 only plain objects convert to a dict"
            )));
        }
    }
    let names = Object::keys(&object)?;

    let mut map: IndexMap<ValueKey, Value> = IndexMap::new();
    for name in names {
        let property: Unknown = object.get_named_property(&name)?;
        map.insert(ValueKey::String(name.as_str().into()), js_to_value(env, property)?);
    }
    Ok(Value::Dict(map))
}

/// Derive a dict key, deferring to the evaluator's own coercion.
///
/// Not re-implemented here: the folding rules are subtle (an integral float
/// shares a slot with the equal int, matching CPython's `hash(2.0) == hash(2)`)
/// and a second copy would drift, producing dicts with two equal-but-distinct
/// keys that silently corrupt `in` / `len` / lookup.
fn to_key(value: &Value) -> Result<ValueKey> {
    value.to_key().map_err(|e| type_error(format!("invalid dict key: {e}")))
}
