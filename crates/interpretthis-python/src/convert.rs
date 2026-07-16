// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Value` <-> Python object conversion.
//!
//! Native, not JSON. `Value::to_json` exists and is tempting, but it is lossy in
//! ways that would surface as user-visible bugs here: `BigInt` stringifies,
//! `Bytes` becomes an array of ints, `List`/`Tuple`/`Set` all collapse to one
//! array, non-string dict keys are `Display`-stringified, and its catch-all arm
//! turns `Decimal`, `Date`, and every user object into `null`. Python has an
//! exact analogue for nearly every portable variant, so we use it.
//!
//! What has no analogue — a sandbox function, class, instance, generator, or
//! pending tool proxy — raises `TypeError` naming the variant. It never
//! degrades to `None`: silently handing back `None` for a value that *is*
//! something turns a boundary error into a wrong answer several lines later.
//!
//! # Copy, not alias
//!
//! `Value::List` is an `Arc<Mutex<Vec<Value>>>` with CPython reference identity
//! *inside* the sandbox. Crossing this boundary **copies**. A list a host passes
//! in as a variable is not aliased by sandboxed code, and mutations the script
//! makes are not visible to the caller's object — read results back with
//! `get_variable`. The alternative (aliasing host objects into the sandbox)
//! would hand sandboxed code a live handle on host memory, which is precisely
//! what this interpreter exists to prevent.

use bigdecimal::BigDecimal;
use chrono::{FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use indexmap::IndexMap;
use interpretthis::{DecimalKind, Value, ValueKey, shared_bytes, shared_list};
use num_bigint::BigInt;
use num_rational::BigRational;
use pyo3::{
    exceptions::PyTypeError,
    prelude::*,
    types::{
        PyBool, PyByteArray, PyBytes, PyDict, PyFloat, PyFrozenSet, PyInt, PyList, PySet, PyString,
        PyTuple,
    },
};

/// Maximum nesting depth when converting a value across the FFI boundary. A
/// cyclic value (`a = []; a.append(a)`, constructible inside the sandbox) or a
/// pathologically deep one would otherwise recurse until the native stack
/// overflows and *aborts the host process*; this bound turns that into a clean
/// `TypeError`. Kept well below what a normal thread stack holds so the guard
/// itself needs no stack-growth machinery — `stacker` is unreliable on musl
/// (it probes the stack via glibc's `pthread_getattr_np`), and a host↔sandbox
/// value nested past this is already pathological.
const MAX_CONVERT_DEPTH: usize = 256;

thread_local! {
    static CONVERT_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// RAII depth counter shared by both conversion directions. Conversion is
/// single-threaded per call (pyo3 holds the GIL), so a thread-local is the
/// right scope, and it works even though the recursion re-enters through
/// pyo3's own trait machinery.
struct DepthGuard;

impl DepthGuard {
    fn enter() -> PyResult<Self> {
        CONVERT_DEPTH.with(|d| {
            if d.get() >= MAX_CONVERT_DEPTH {
                return Err(PyTypeError::new_err(
                    "interpretthis: value is nested too deeply to convert across the sandbox \
                     boundary (a reference cycle, or excessive nesting)",
                ));
            }
            d.set(d.get() + 1);
            Ok(Self)
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        CONVERT_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// A `Value` the sandbox can hold but Python cannot receive.
fn unsupported_outbound(value: &Value) -> PyErr {
    PyTypeError::new_err(format!(
        "interpretthis: cannot convert sandbox value of type '{}' to a Python object; \
         it exists only inside the interpreter",
        value.type_name()
    ))
}

/// A Python object the sandbox cannot accept.
fn unsupported_inbound(ob: &Bound<'_, PyAny>) -> PyErr {
    let type_name = ob
        .get_type()
        .name()
        .map_or_else(|_| "?".to_string(), |name| name.to_string_lossy().into_owned());
    PyTypeError::new_err(format!(
        "interpretthis: cannot pass a Python object of type '{type_name}' into the sandbox; \
         supported types are None, bool, int, float, str, bytes, list, tuple, set, frozenset, \
         dict, range, Decimal, Fraction, date, datetime, time, timedelta and timezone"
    ))
}

// ---------------------------------------------------------------------------
// Value -> Python
// ---------------------------------------------------------------------------

/// Convert an interpreter [`Value`] into a Python object.
///
/// # Errors
///
/// Returns `TypeError` for variants with no Python analogue (functions,
/// classes, instances, generators, pending tool proxies, ...).
pub fn value_to_py<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    let _depth = DepthGuard::enter()?;
    value_to_py_inner(py, value)
}

fn value_to_py_inner<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    Ok(match value {
        Value::None => py.None().into_bound(py),
        Value::NotImplemented => py.NotImplemented().into_bound(py),
        Value::Bool(b) => b.into_pyobject(py)?.to_owned().into_any(),
        Value::Int(i) => i.into_pyobject(py)?.into_any(),
        Value::BigInt(i) => (**i).clone().into_pyobject(py)?.into_any(),
        Value::Float(f) => f.into_pyobject(py)?.into_any(),
        Value::String(s) => s.as_str().into_pyobject(py)?.into_any(),
        Value::Bytes(b) => PyBytes::new(py, b).into_any(),
        Value::ByteArray(b) => PyByteArray::new(py, &b.lock()).into_any(),
        // A memoryview projects to bytes across the boundary (its buffer view
        // isn't representable host-side).
        Value::MemoryView(_) => {
            PyBytes::new(py, &interpretthis::memoryview_bytes(value)).into_any()
        }

        // `array.array` crosses the boundary as a plain Python list (its exact
        // element values), which is the natural lossy-but-usable form.
        Value::List(items) | Value::Array { items, .. } => {
            // Lock scope ends before the list is built, so a tool callback that
            // re-enters the interpreter cannot deadlock against this guard.
            let snapshot: Vec<Value> = items.lock().clone();
            let converted =
                snapshot.iter().map(|v| value_to_py(py, v)).collect::<PyResult<Vec<_>>>()?;
            PyList::new(py, converted)?.into_any()
        }
        Value::Tuple(items) => {
            let converted =
                items.iter().map(|v| value_to_py(py, v)).collect::<PyResult<Vec<_>>>()?;
            PyTuple::new(py, converted)?.into_any()
        }
        Value::Set(_) => {
            let items = value.set_items().unwrap_or_default();
            let converted =
                items.iter().map(|v| value_to_py(py, v)).collect::<PyResult<Vec<_>>>()?;
            PySet::new(py, converted)?.into_any()
        }
        Value::Frozenset(_) => {
            let items = value.set_items().unwrap_or_default();
            let converted =
                items.iter().map(|v| value_to_py(py, v)).collect::<PyResult<Vec<_>>>()?;
            PyFrozenSet::new(py, converted)?.into_any()
        }
        // A live dict view materialises to a list of its elements at the
        // boundary (CPython's dict_keys/values/items aren't representable
        // as a distinct pyo3 type here; a list preserves iteration).
        Value::DictView { .. } => {
            let items = value.dict_view_elements().unwrap_or_default();
            let converted =
                items.iter().map(|v| value_to_py(py, v)).collect::<PyResult<Vec<_>>>()?;
            PyList::new(py, converted)?.into_any()
        }
        Value::Dict(map) | Value::OrderedDict(map) => {
            let dict = PyDict::new(py);
            let snapshot = map.lock().clone();
            for (key, val) in &snapshot {
                // Keys round-trip through `ValueKey::to_value`, so a folded
                // integral-float key (`{2.0: x}` is stored as `Int(2)`) comes
                // back as `2` — the documented, deliberate coercion.
                dict.set_item(value_to_py(py, &key.to_value())?, value_to_py(py, val)?)?;
            }
            dict.into_any()
        }

        Value::Range { start, stop, step } => {
            py.import("builtins")?.getattr("range")?.call1((*start, *stop, *step))?
        }

        Value::Complex(c) => pyo3::types::PyComplex::from_doubles(py, c.re, c.im).into_any(),
        Value::Ellipsis => py.import("builtins")?.getattr("Ellipsis")?,

        Value::Decimal(d, kind) => {
            if *kind == DecimalKind::Normal {
                (**d).clone().into_pyobject(py)?.into_any()
            } else {
                // pyo3's BigDecimal cannot emit a signed zero or Infinity/NaN;
                // rebuild from the exact `str()` form via decimal.Decimal(str).
                let s = Value::Decimal(d.clone(), *kind).to_string();
                py.import("decimal")?.getattr("Decimal")?.call1((s,))?
            }
        }
        Value::Fraction(f) => (**f).clone().into_pyobject(py)?.into_any(),

        Value::Date(d) => d.into_pyobject(py)?.into_any(),
        Value::Time(t) => t.into_pyobject(py)?.into_any(),
        Value::DateTime { dt, tz_offset_secs } => match tz_offset_secs {
            Some(secs) => {
                let offset = FixedOffset::east_opt(*secs).ok_or_else(|| {
                    PyTypeError::new_err(format!("invalid UTC offset: {secs} seconds"))
                })?;
                dt.and_local_timezone(offset)
                    .single()
                    .ok_or_else(|| {
                        PyTypeError::new_err("datetime is not representable at that UTC offset")
                    })?
                    .into_pyobject(py)?
                    .into_any()
            }
            None => dt.into_pyobject(py)?.into_any(),
        },
        Value::TimeDelta(micros) => TimeDelta::microseconds(*micros).into_pyobject(py)?.into_any(),
        Value::TimeZone(secs) => FixedOffset::east_opt(*secs)
            .ok_or_else(|| PyTypeError::new_err(format!("invalid UTC offset: {secs} seconds")))?
            .into_pyobject(py)?
            .into_any(),

        // Value is #[non_exhaustive]; everything else is interpreter-internal
        // (Function, Lambda, Class, Instance, Generator, LazyProxy, Counter, ...).
        other => return Err(unsupported_outbound(other)),
    })
}

// ---------------------------------------------------------------------------
// Python -> Value
// ---------------------------------------------------------------------------

/// Convert a Python object into an interpreter [`Value`].
///
/// # Errors
///
/// Returns `TypeError` for Python objects with no sandbox analogue, and for
/// unhashable dict/set keys.
pub fn py_to_value(ob: &Bound<'_, PyAny>) -> PyResult<Value> {
    let _depth = DepthGuard::enter()?;
    py_to_value_inner(ob)
}

fn py_to_value_inner(ob: &Bound<'_, PyAny>) -> PyResult<Value> {
    if ob.is_none() {
        return Ok(Value::None);
    }
    // The Ellipsis singleton (`...`) is distinct from None.
    if ob.is(&py_ellipsis(ob.py())?) {
        return Ok(Value::Ellipsis);
    }
    // complex BEFORE the numeric primitives (it is neither int nor float).
    if let Ok(c) = ob.cast::<pyo3::types::PyComplex>() {
        return Ok(Value::Complex(Box::new(num_complex::Complex64::new(c.real(), c.imag()))));
    }

    // bool BEFORE int: in Python `bool` is a subclass of `int`, so an `isinstance`
    // check against int would swallow True/False and turn them into 1/0.
    if ob.is_instance_of::<PyBool>() {
        return Ok(Value::Bool(ob.extract::<bool>()?));
    }
    if ob.is_instance_of::<PyInt>() {
        // Fast path for the common case; promote only when it does not fit,
        // which mirrors the interpreter's own hybrid int representation.
        return Ok(match ob.extract::<i64>() {
            Ok(i) => Value::Int(i),
            Err(_) => Value::BigInt(Box::new(ob.extract::<BigInt>()?)),
        });
    }
    if ob.is_instance_of::<PyFloat>() {
        return Ok(Value::Float(ob.extract::<f64>()?));
    }
    if ob.is_instance_of::<PyString>() {
        return Ok(Value::String(ob.extract::<String>()?.into()));
    }
    if ob.is_instance_of::<PyByteArray>() {
        return Ok(Value::ByteArray(shared_bytes(ob.extract::<Vec<u8>>()?)));
    }
    if ob.is_instance_of::<PyBytes>() {
        return Ok(Value::Bytes(ob.extract::<Vec<u8>>()?));
    }

    if let Ok(list) = ob.cast::<PyList>() {
        let items = list.iter().map(|v| py_to_value(&v)).collect::<PyResult<Vec<_>>>()?;
        return Ok(Value::List(shared_list(items)));
    }
    if let Ok(tuple) = ob.cast::<PyTuple>() {
        let items = tuple.iter().map(|v| py_to_value(&v)).collect::<PyResult<Vec<_>>>()?;
        return Ok(Value::Tuple(items));
    }
    if let Ok(set) = ob.cast::<PySet>() {
        let items = set.iter().map(|v| py_to_value(&v)).collect::<PyResult<Vec<_>>>()?;
        return Ok(Value::new_set(items));
    }
    if let Ok(set) = ob.cast::<PyFrozenSet>() {
        let items = set.iter().map(|v| py_to_value(&v)).collect::<PyResult<Vec<_>>>()?;
        return Ok(Value::new_frozenset(items));
    }
    if let Ok(dict) = ob.cast::<PyDict>() {
        let mut map: IndexMap<ValueKey, Value> = IndexMap::new();
        for (key, val) in dict {
            let key = py_to_value(&key)?;
            // Delegates to the evaluator's own key coercion rather than
            // re-deriving it here; a second implementation would drift.
            let key = key.to_key().map_err(|e| PyTypeError::new_err(e.to_string()))?;
            map.insert(key, py_to_value(&val)?);
        }
        return Ok(Value::Dict(interpretthis::shared_dict(map)));
    }

    // datetime BEFORE date: `datetime.datetime` subclasses `datetime.date`, so
    // checking date first would truncate every datetime to midnight.
    if let Ok(dt) = ob.extract::<chrono::DateTime<FixedOffset>>() {
        return Ok(Value::DateTime {
            dt: dt.naive_local(),
            tz_offset_secs: Some(dt.offset().local_minus_utc()),
        });
    }
    if let Ok(dt) = ob.extract::<NaiveDateTime>() {
        return Ok(Value::DateTime { dt, tz_offset_secs: None });
    }
    if let Ok(d) = ob.extract::<NaiveDate>() {
        return Ok(Value::Date(d));
    }
    // `datetime.time`: a tz-aware time silently lost its tzinfo through the
    // NaiveTime extractor. The interpreter has no tz-aware time, so reject it.
    if is_instance_of(ob, "datetime", "time")? {
        if !ob.getattr("tzinfo")?.is_none() {
            return Err(PyTypeError::new_err(
                "tz-aware datetime.time is not supported (the interpreter models only naive time)",
            ));
        }
        return Ok(Value::Time(ob.extract::<NaiveTime>()?));
    }
    if let Ok(delta) = ob.extract::<TimeDelta>() {
        return Ok(Value::TimeDelta(delta.num_microseconds().ok_or_else(|| {
            PyTypeError::new_err("timedelta is too large to represent in microseconds")
        })?));
    }
    if let Ok(offset) = ob.extract::<FixedOffset>() {
        return Ok(Value::TimeZone(offset.local_minus_utc()));
    }

    // Decimal / Fraction: gate on the *actual* type first. pyo3's BigDecimal /
    // BigRational extractors are `str(obj)`-parse / duck-typed, so without the
    // gate any object with a numeric `__str__` or `.numerator`/`.denominator`
    // (numpy.int64, a Money class) would be silently reinterpreted.
    if is_instance_of(ob, "decimal", "Decimal")? {
        // Detect Infinity / NaN from the string form first — `bigdecimal` can't
        // parse them (or emit a signed zero, recovered below).
        let s = ob.str()?.to_str()?.to_string();
        let trimmed = s.trim();
        let lower = trimmed.to_ascii_lowercase();
        let (neg, body) =
            lower.strip_prefix('-').map_or((false, lower.as_str()), |rest| (true, rest));
        if body == "inf" || body == "infinity" {
            let kind = if neg { DecimalKind::NegInf } else { DecimalKind::PosInf };
            return Ok(Value::Decimal(Box::new(BigDecimal::from(0)), kind));
        }
        if body == "nan" || body == "snan" {
            return Ok(Value::Decimal(Box::new(BigDecimal::from(0)), DecimalKind::Nan));
        }
        let big = ob.extract::<BigDecimal>()?;
        // `bigdecimal` drops the sign of a zero; recover CPython's negative
        // zero (`Decimal('-0.0')`) from the repr so it round-trips.
        let neg_zero = bigdecimal::Zero::is_zero(&big) && trimmed.starts_with('-');
        let kind = if neg_zero { DecimalKind::NegZero } else { DecimalKind::Normal };
        return Ok(Value::Decimal(Box::new(big), kind));
    }
    if is_instance_of(ob, "fractions", "Fraction")? {
        return Ok(Value::Fraction(Box::new(ob.extract::<BigRational>()?)));
    }

    if let Some(range) = extract_range(ob)? {
        return Ok(range);
    }

    Err(unsupported_inbound(ob))
}

/// The `Ellipsis` singleton object.
fn py_ellipsis(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    py.import("builtins")?.getattr("Ellipsis")
}

/// Whether `ob` is an instance of `<module>.<name>` (e.g. `decimal.Decimal`).
/// Used to gate str-parse/duck-typed extractors on the real type.
fn is_instance_of(ob: &Bound<'_, PyAny>, module: &str, name: &str) -> PyResult<bool> {
    let ty = ob.py().import(module)?.getattr(name)?;
    ob.is_instance(&ty)
}

/// Recognise a builtin `range` object.
///
/// Matched by exact type rather than by duck-typing on `.start`/`.stop`/`.step`,
/// so an unrelated object that happens to carry those attributes is rejected by
/// the catch-all instead of being silently reinterpreted as a range.
fn extract_range(ob: &Bound<'_, PyAny>) -> PyResult<Option<Value>> {
    let range_type = ob.py().import("builtins")?.getattr("range")?;
    if !ob.is_instance(&range_type)? {
        return Ok(None);
    }
    Ok(Some(Value::Range {
        start: ob.getattr("start")?.extract()?,
        stop: ob.getattr("stop")?.extract()?,
        step: ob.getattr("step")?.extract()?,
    }))
}
