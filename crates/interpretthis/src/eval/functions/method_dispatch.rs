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
                // Preserve the original shared handle when nothing needs
                // resolving, so functions that mutate a list argument in
                // place (`heapq.heapify`, `list.sort` via a callable, …)
                // affect the caller's list — CPython reference semantics.
                // Only rebuild into a fresh Arc when an inner proxy must
                // be resolved.
                if snapshot.iter().any(|v| matches!(v, Value::LazyProxy(_))) {
                    let mut resolved_items = Vec::with_capacity(snapshot.len());
                    for item in &snapshot {
                        resolved_items.push(resolve_proxy(item).await?);
                    }
                    resolved_args.push(Value::List(shared_list(resolved_items)));
                } else {
                    resolved_args.push(Value::List(items));
                }
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

// ---------------------------------------------------------------------------
// Per-type method handlers (fn-pointer table)
// ---------------------------------------------------------------------------

/// Signature of a builtin method-table entry.
type MethodsHandler =
    fn(&mut Value, &str, &[Value], &IndexMap<String, Value>) -> Result<MethodOutcome, EvalError>;

fn str_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::String(s) = obj else {
        return Err(type_mismatch("str"));
    };
    methods::str::dispatch_string_method(s, method, args, kwargs).map(MethodOutcome::pure)
}

fn list_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::List(items) = obj else {
        return Err(type_mismatch("list"));
    };
    let mut guard = items.lock();
    methods::list::dispatch_list_method(&mut guard, method, args, kwargs)
}

fn array_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    use crate::eval::modules::array_mod::{coerce_element, itemsize};
    let Value::Array { typecode, items } = obj else {
        return Err(type_mismatch("array.array"));
    };
    let typecode = *typecode;
    match method {
        // Mutators that add elements coerce each to the array's element kind.
        "append" => {
            let coerced = coerce_element(typecode, arg1(method, args)?)?;
            let size = crate::state::estimate_value_size(&coerced);
            items.lock().push(coerced);
            Ok(MethodOutcome::grew(Value::None, size))
        }
        "extend" | "fromlist" => {
            let elems = crate::eval::control_flow::iterate_value(arg1(method, args)?)?;
            let mut guard = items.lock();
            let mut added = 0;
            for e in elems {
                let c = coerce_element(typecode, &e)?;
                added += crate::state::estimate_value_size(&c);
                guard.push(c);
            }
            Ok(MethodOutcome::grew(Value::None, added))
        }
        "insert" => {
            let idx = crate::eval::functions::value_to_i64(arg1(method, args)?)?;
            let value = coerce_element(
                typecode,
                args.get(1).ok_or_else(|| {
                    EvalError::from(InterpreterError::TypeError(
                        "insert() takes exactly 2 arguments".into(),
                    ))
                })?,
            )?;
            let size = crate::state::estimate_value_size(&value);
            let mut guard = items.lock();
            // Clamp the index into `[0, len]` the way list.insert does.
            let len = guard.len() as i64;
            let pos = if idx < 0 { (len + idx).max(0) } else { idx.min(len) } as usize;
            guard.insert(pos, value);
            Ok(MethodOutcome::grew(Value::None, size))
        }
        // A typed list copy.
        "tolist" => {
            let guard = items.lock();
            Ok(MethodOutcome::pure(Value::List(crate::value::shared_list(guard.clone()))))
        }
        // `(address, length)` — no real buffer, so the address is 0.
        "buffer_info" => {
            let len = items.lock().len() as i64;
            Ok(MethodOutcome::pure(Value::Tuple(vec![Value::Int(0), Value::Int(len)])))
        }
        "__len__" => Ok(MethodOutcome::pure(Value::Int(crate::eval::functions::to_len_i64(
            items.lock().len(),
        )?))),
        // pop/remove/index/count/reverse behave exactly as on the element list.
        "pop" | "remove" | "index" | "count" | "reverse" => {
            let _ = itemsize; // itemsize is exposed as an attribute, not a method
            let mut guard = items.lock();
            methods::list::dispatch_list_method(&mut guard, method, args, kwargs)
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'array.array' object has no attribute '{method}'"
        ))
        .into()),
    }
}

fn dict_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Dict(map) = obj else {
        return Err(type_mismatch("dict"));
    };
    // `keys`/`values`/`items` return a LIVE view over the shared dict
    // (reflects later mutations, and keys/items are set-like) rather
    // than a materialised list.
    if let Some(kind) = match method {
        "keys" => Some(crate::value::DictViewKind::Keys),
        "values" => Some(crate::value::DictViewKind::Values),
        "items" => Some(crate::value::DictViewKind::Items),
        _ => None,
    } {
        reject_kwargs(method, kwargs)?;
        if !args.is_empty() {
            return Err(InterpreterError::TypeError(format!(
                "{method}() takes no arguments ({} given)",
                args.len()
            ))
            .into());
        }
        return Ok(MethodOutcome::pure(Value::DictView { dict: map.clone(), kind }));
    }
    // The dict methods are sync and mutate through the guard, so
    // holding the lock across the call is deadlock-free and the shared
    // dict observes the mutation.
    let mut guard = map.lock();
    methods::dict::dispatch_dict_method(&mut guard, method, args, kwargs)
}

fn counter_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Counter(map) = obj else {
        return Err(type_mismatch("Counter"));
    };
    methods::counter::dispatch_counter_method(map, method, args, kwargs)
}

fn deque_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Deque { items, maxlen } = obj else {
        return Err(type_mismatch("deque"));
    };
    methods::deque::dispatch_deque_method(items, maxlen.as_ref(), method, args, kwargs)
}

fn defaultdict_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::DefaultDict(data) = obj else {
        return Err(type_mismatch("defaultdict"));
    };
    methods::dict::dispatch_dict_method(&mut data.items, method, args, kwargs)
}

fn template_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Template(template) = obj else {
        return Err(type_mismatch("Template"));
    };
    match method {
        // `substitute` raises on a missing key / bad placeholder;
        // `safe_substitute` leaves them in place.
        "substitute" | "safe_substitute" => {
            let safe = method == "safe_substitute";
            let rendered =
                super::super::strings::template_substitute(template, args, kwargs, safe)?;
            Ok(MethodOutcome::pure(Value::String(rendered.into())))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'string.Template' object has no attribute '{method}'"
        ))
        .into()),
    }
}

fn chainmap_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::ChainMap(maps) = obj else {
        return Err(type_mismatch("ChainMap"));
    };
    match method {
        // `new_child(m=None)` prepends `m` (or a fresh empty dict).
        "new_child" => {
            let child = match args.first() {
                Some(v @ Value::Dict(_)) => v.clone(),
                None | Some(Value::None) => Value::Dict(crate::value::shared_dict(IndexMap::new())),
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "ChainMap.new_child() argument must be a mapping, not '{}'",
                        other.type_name()
                    ))
                    .into());
                }
            };
            let mut new_maps = Vec::with_capacity(maps.len() + 1);
            new_maps.push(child);
            new_maps.extend(maps.iter().cloned());
            Ok(MethodOutcome::pure(Value::ChainMap(new_maps)))
        }
        // `copy()` copies the first map, sharing the rest (CPython).
        "copy" => {
            let mut new_maps = maps.clone();
            let copied = match new_maps.first() {
                Some(Value::Dict(first)) => Some(crate::value::shared_dict(first.lock().clone())),
                _ => None,
            };
            if let Some(c) = copied {
                new_maps[0] = Value::Dict(c);
            }
            Ok(MethodOutcome::pure(Value::ChainMap(new_maps)))
        }
        // Read-only views search all maps (first-map value wins).
        "keys" | "values" | "items" | "get" | "__contains__" => {
            let mut merged = crate::types::chainmap_contents(maps);
            methods::dict::dispatch_dict_method(&mut merged, method, args, kwargs)
        }
        // Mutating methods (pop/popitem/clear/setdefault/update/…)
        // target the first map, matching CPython.
        _ => {
            if let Some(Value::Dict(first)) = maps.first() {
                let mut guard = first.lock();
                methods::dict::dispatch_dict_method(&mut guard, method, args, kwargs)
            } else {
                Err(InterpreterError::AttributeError(format!(
                    "'ChainMap' object has no attribute '{method}'"
                ))
                .into())
            }
        }
    }
}

fn set_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Set(items) = obj else {
        return Err(type_mismatch("set"));
    };
    methods::set::dispatch_set_method(items, method, args, kwargs)
}

fn frozenset_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Frozenset(items) = obj else {
        return Err(type_mismatch("frozenset"));
    };
    methods::set::dispatch_frozenset_method(items, method, args, kwargs)
}

fn tuple_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Tuple(items) = obj else {
        return Err(type_mismatch("tuple"));
    };
    methods::tuple::dispatch_tuple_method(items, method, args, kwargs).map(MethodOutcome::pure)
}

fn int_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    // `to_bytes` needs the full value (large ints included), so handle it here
    // before the i64 narrowing below would reject a BigInt receiver.
    if method == "to_bytes" {
        let value = match obj {
            Value::Int(i) => num_bigint::BigInt::from(*i),
            Value::BigInt(b) => (**b).clone(),
            Value::Bool(b) => num_bigint::BigInt::from(i64::from(*b)),
            _ => return Err(type_mismatch("int")),
        };
        return super::helpers::int_to_bytes(&value, args, kwargs).map(MethodOutcome::pure);
    }
    match obj {
        Value::Int(i) => {
            methods::int::dispatch_int_method(*i, method, args, kwargs).map(MethodOutcome::pure)
        }
        Value::BigInt(i) => match i64::try_from(i.as_ref()) {
            Ok(n) => {
                methods::int::dispatch_int_method(n, method, args, kwargs).map(MethodOutcome::pure)
            }
            // Beyond i64: stay in arbitrary precision so
            // `bit_length`/`__index__`/`__abs__`/... don't raise a
            // spurious OverflowError from narrowing.
            Err(_) => methods::int::dispatch_bigint_method(i, method, args, kwargs)
                .map(MethodOutcome::pure),
        },
        _ => Err(type_mismatch("int")),
    }
}

fn float_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Float(f) = obj else { return Err(type_mismatch("float")) };
    methods::float::dispatch_float_method(*f, method, args, kwargs).map(MethodOutcome::pure)
}

/// `complex` methods: `conjugate()` (and `real`/`imag` for parity with `int`,
/// though those are normally read as attributes). All are argument-less.
fn complex_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Complex(c) = obj else { return Err(type_mismatch("complex")) };
    reject_kwargs(method, kwargs)?;
    if !args.is_empty() {
        return Err(InterpreterError::TypeError(format!("{method}() takes no arguments")).into());
    }
    match method {
        "conjugate" => Ok(MethodOutcome::pure(Value::Complex(Box::new(c.conj())))),
        "real" => Ok(MethodOutcome::pure(Value::Float(c.re))),
        "imag" => Ok(MethodOutcome::pure(Value::Float(c.im))),
        _ => Err(InterpreterError::AttributeError(format!(
            "'complex' object has no attribute '{method}'"
        ))
        .into()),
    }
}

fn bytes_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Bytes(b) = obj else {
        return Err(type_mismatch("bytes"));
    };
    methods::bytes::dispatch_bytes_method(b, method, args, kwargs).map(MethodOutcome::pure)
}

fn bytearray_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::ByteArray(b) = obj else {
        return Err(type_mismatch("bytearray"));
    };
    methods::bytes::dispatch_bytearray_method(b, method, args, kwargs)
}

fn memoryview_methods(
    obj: &mut Value,
    method: &str,
    _args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::MemoryView(_) = obj else {
        return Err(type_mismatch("memoryview"));
    };
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    let raw = crate::types::memoryview_bytes(obj);
    methods::bytes::dispatch_memoryview_method(&raw, method).map(MethodOutcome::pure)
}

fn date_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Date(date) = obj else {
        return Err(type_mismatch("date"));
    };
    crate::eval::modules::datetime::dispatch_date_method(*date, method, args, kwargs)
        .map(MethodOutcome::pure)
}

fn datetime_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::DateTime { dt, tz_offset_secs } = obj else {
        return Err(type_mismatch("datetime"));
    };
    crate::eval::modules::datetime::dispatch_datetime_method(
        *dt,
        *tz_offset_secs,
        method,
        args,
        kwargs,
    )
    .map(MethodOutcome::pure)
}

fn time_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Time(t) = obj else {
        return Err(type_mismatch("time"));
    };
    crate::eval::modules::datetime::dispatch_time_method(*t, method, args, kwargs)
        .map(MethodOutcome::pure)
}

fn timedelta_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::TimeDelta(micros) = obj else {
        return Err(type_mismatch("timedelta"));
    };
    crate::eval::modules::datetime::dispatch_timedelta_method(*micros, method, args, kwargs)
        .map(MethodOutcome::pure)
}

fn re_match_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::ReMatch(m) = obj else {
        return Err(type_mismatch("re.Match"));
    };
    crate::eval::modules::re::dispatch_match_method(m, method, args, kwargs)
        .map(MethodOutcome::pure)
}

fn re_pattern_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::RePattern(p) = obj else {
        return Err(type_mismatch("re.Pattern"));
    };
    crate::eval::modules::re::dispatch_pattern_method(p, method, args, kwargs)
        .map(MethodOutcome::pure)
}

fn fraction_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Fraction(f) = obj else {
        return Err(type_mismatch("Fraction"));
    };
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "limit_denominator" => {
            let max_denom = match args.first() {
                None => num_bigint::BigInt::from(1_000_000),
                Some(v) => crate::value::value_as_bigint(v).ok_or_else(|| {
                    EvalError::from(InterpreterError::TypeError(
                        "limit_denominator() argument must be an integer".into(),
                    ))
                })?,
            };
            Ok(MethodOutcome::pure(Value::Fraction(Box::new(limit_denominator(f, &max_denom)))))
        }
        "as_integer_ratio" => Ok(MethodOutcome::pure(Value::Tuple(vec![
            crate::value::int_from_bigint(f.numer().clone()),
            crate::value::int_from_bigint(f.denom().clone()),
        ]))),
        "__floor__" | "__ceil__" | "__trunc__" => {
            let r = match method {
                "__floor__" => f.floor(),
                "__ceil__" => f.ceil(),
                _ => f.trunc(),
            };
            Ok(MethodOutcome::pure(crate::value::int_from_bigint(r.to_integer())))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'Fraction' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// CPython's `Fraction.limit_denominator` — the closest fraction with a
/// denominator not exceeding `max_denominator`, via the continued-fraction
/// convergents.
fn limit_denominator(
    f: &num_rational::BigRational,
    max_denominator: &num_bigint::BigInt,
) -> num_rational::BigRational {
    use num_bigint::BigInt;
    use num_rational::BigRational;
    use num_traits::{One as _, Signed as _, Zero as _};
    if max_denominator < &BigInt::one() {
        return f.clone();
    }
    if f.denom() <= max_denominator {
        return f.clone();
    }
    let (mut p0, mut q0, mut p1, mut q1) =
        (BigInt::zero(), BigInt::one(), BigInt::one(), BigInt::zero());
    let (mut n, mut d) = (f.numer().clone(), f.denom().clone());
    loop {
        let a = &n / &d;
        let q2 = &q0 + &a * &q1;
        if &q2 > max_denominator {
            break;
        }
        let new_p1 = &p0 + &a * &p1;
        p0 = std::mem::replace(&mut p1, new_p1);
        q0 = std::mem::replace(&mut q1, q2);
        let new_d = &n - &a * &d;
        n = std::mem::replace(&mut d, new_d);
    }
    let k = (max_denominator - &q0) / &q1;
    let bound1 = BigRational::new(&p0 + &k * &p1, &q0 + &k * &q1);
    let bound2 = BigRational::new(p1, q1);
    if (&bound2 - f).abs() <= (&bound1 - f).abs() { bound2 } else { bound1 }
}

fn decimal_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::Decimal(d, _) = obj else {
        return Err(type_mismatch("Decimal"));
    };
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    crate::eval::modules::decimal::dispatch_decimal_method(d, method, args).map(MethodOutcome::pure)
}

fn hash_digest_methods(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    let Value::HashDigest { bytes, .. } = obj else {
        return Err(type_mismatch("HASH"));
    };
    // `update(data)` appends to the accumulated buffer (the digest is computed
    // lazily), so the incremental create-then-update pattern works.
    if method == "update" {
        crate::eval::functions::reject_kwargs(method, kwargs)?;
        let data = match args.first() {
            Some(Value::Bytes(b)) => b.clone(),
            Some(Value::ByteArray(b)) => b.lock().clone(),
            _ => {
                return Err(InterpreterError::TypeError(
                    "update() argument must be a bytes-like object".into(),
                )
                .into());
            }
        };
        let grew = data.len();
        bytes.extend_from_slice(&data);
        return Ok(MethodOutcome::grew(Value::None, grew));
    }
    let Value::HashDigest { algo, bytes } = obj else {
        return Err(type_mismatch("HASH"));
    };
    crate::eval::modules::hashlib::dispatch_hash_method(algo, bytes, method, args, kwargs)
        .map(MethodOutcome::pure)
}

fn type_mismatch(expected: &str) -> EvalError {
    InterpreterError::TypeError(format!("internal: method table expected {expected}")).into()
}

/// Look up the method-table handler for `obj`'s runtime type.
fn methods_handler_for(obj: &Value) -> Option<MethodsHandler> {
    match obj {
        Value::String(_) => Some(str_methods),
        Value::List(_) => Some(list_methods),
        Value::Array { .. } => Some(array_methods),
        Value::Dict(_) => Some(dict_methods),
        Value::Counter(_) => Some(counter_methods),
        Value::Deque { .. } => Some(deque_methods),
        Value::DefaultDict(_) => Some(defaultdict_methods),
        Value::ChainMap(_) => Some(chainmap_methods),
        Value::Template(_) => Some(template_methods),
        Value::Set(_) => Some(set_methods),
        Value::Frozenset(_) => Some(frozenset_methods),
        Value::Tuple(_) => Some(tuple_methods),
        Value::Int(_) | Value::BigInt(_) => Some(int_methods),
        Value::Float(_) => Some(float_methods),
        Value::Complex(_) => Some(complex_methods),
        Value::Bytes(_) => Some(bytes_methods),
        Value::ByteArray(_) => Some(bytearray_methods),
        Value::MemoryView(_) => Some(memoryview_methods),
        Value::Date(_) => Some(date_methods),
        Value::DateTime { .. } => Some(datetime_methods),
        Value::Time(_) => Some(time_methods),
        Value::TimeDelta(_) => Some(timedelta_methods),
        Value::ReMatch(_) => Some(re_match_methods),
        Value::RePattern(_) => Some(re_pattern_methods),
        Value::Decimal(..) => Some(decimal_methods),
        Value::Fraction(_) => Some(fraction_methods),
        Value::HashDigest { .. } => Some(hash_digest_methods),
        _ => None,
    }
}

/// Dispatch a method call against a mutable receiver slot.
///
/// Table-driven: each method-bearing builtin has a dedicated handler
/// (see [`methods_handler_for`]). Read-only methods return a fresh value
/// (`mem_delta == 0`); mutating methods modify `obj` in place and report
/// the byte delta. `args` / `kwargs` must already be proxy-resolved
/// (see [`resolve_method_args`] / [`resolve_method_kwargs`]).
/// Map a reflective builtin dunder call (`[1, 2].__len__()`, `"ab".__getitem__(0)`)
/// to the sync operator it wraps. Returns `Ok(Some(_))` when handled,
/// `Ok(None)` to fall through to the type's method table (and its
/// AttributeError for a genuinely absent dunder). Only the sync-computable
/// dunders are covered; ones needing `&mut state` (`__iter__`, `__add__`, …)
/// are left to the normal call path.
fn try_builtin_dunder(
    obj: &Value,
    method: &str,
    args: &[Value],
) -> Result<Option<MethodOutcome>, EvalError> {
    let pure = |v: Value| Ok(Some(MethodOutcome::pure(v)));
    match method {
        // Only expose `__len__` on sized types (int has none), so a failure
        // falls through to AttributeError rather than surfacing a len error.
        "__len__" => match crate::types::dispatch_len(obj) {
            Ok(n) => pure(Value::Int(crate::eval::functions::to_len_i64(n)?)),
            Err(_) => Ok(None),
        },
        "__contains__" => match crate::types::dispatch_contains(obj, arg1(method, args)?) {
            Ok(b) => pure(Value::Bool(b)),
            Err(_) => Ok(None),
        },
        "__getitem__" => match crate::types::dispatch_getitem(obj, arg1(method, args)?) {
            Ok(v) => pure(v),
            Err(e) => Err(e),
        },
        "__str__" => pure(Value::String(format!("{obj}").into())),
        "__repr__" => pure(Value::String(obj.repr().into())),
        "__bool__" => pure(Value::Bool(obj.is_truthy())),
        // `__floor__`/`__ceil__`/`__trunc__` return the integral part per the
        // numeric type (exact for Fraction/Decimal/int, truncating floor/ceil
        // for float). Non-numeric types fall through to AttributeError.
        "__floor__" | "__ceil__" | "__trunc__" => match numeric_integral(obj, method) {
            Some(v) => pure(v),
            None => Ok(None),
        },
        // `x.__round__()` returns an int (round-half-to-even); `x.__round__(n)`
        // returns the same numeric type rounded to that scale. Mirrors the
        // `round()` builtin so both surfaces agree.
        "__round__" => {
            use crate::eval::functions::{
                round_bigint, round_decimal, round_float, round_fraction, round_int, value_to_i64,
            };
            let ndigits = match args.first() {
                Some(v) => Some(value_to_i64(v)?),
                None => None,
            };
            match obj {
                Value::Int(i) => pure(round_int(*i, ndigits)),
                Value::BigInt(b) => pure(crate::value::int_from_bigint(round_bigint(b, ndigits))),
                Value::Bool(b) => pure(round_int(i64::from(*b), ndigits)),
                Value::Float(f) => pure(round_float(*f, ndigits)?),
                Value::Decimal(d, _) => pure(round_decimal(d, ndigits)),
                Value::Fraction(fr) => pure(round_fraction(fr, ndigits)),
                _ => Ok(None),
            }
        }
        // Binary arithmetic / bitwise dunders on builtins (`(10).__add__(5)`,
        // `(100).__divmod__(7)`). Routed through the shared sync binop with
        // default context (prec 28, 1 Mibit) — the only divergence is a Decimal
        // op made through an explicit dunder while a non-default getcontext()
        // precision is active, which is vanishingly rare. Instances keep their
        // own user-defined dunders (handled before this shim is reached).
        _ if !matches!(obj, Value::Instance(_) | Value::Class(_)) => {
            let Some((op, reflected)) = arith_dunder_op(method) else {
                return Ok(None);
            };
            let Some(other) = args.first() else {
                return Ok(None);
            };
            let (lhs, rhs) = if reflected { (other, obj) } else { (obj, other) };
            if method == "__divmod__" || method == "__rdivmod__" {
                let q = crate::eval::operations::apply_binop(
                    lhs,
                    rhs,
                    rustpython_parser::ast::Operator::FloorDiv,
                    28,
                    1_048_576,
                )?;
                let r = crate::eval::operations::apply_binop(
                    lhs,
                    rhs,
                    rustpython_parser::ast::Operator::Mod,
                    28,
                    1_048_576,
                )?;
                return pure(Value::Tuple(vec![q, r]));
            }
            match crate::eval::operations::apply_binop(lhs, rhs, op, 28, 1_048_576) {
                Ok(v) => pure(v),
                // A type mismatch means this dunder doesn't apply to the pair;
                // fall through to AttributeError rather than surfacing it.
                Err(_) => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

/// Map a binary arithmetic/bitwise dunder name to its operator and whether it
/// is the reflected form (operands swapped). `__divmod__`/`__rdivmod__` return a
/// placeholder operator (`Add`) — the caller special-cases them.
fn arith_dunder_op(method: &str) -> Option<(rustpython_parser::ast::Operator, bool)> {
    use rustpython_parser::ast::Operator::{
        Add, BitAnd, BitOr, BitXor, Div, FloorDiv, LShift, MatMult, Mod, Mult, Pow, RShift, Sub,
    };
    let (op, reflected) = match method {
        "__add__" => (Add, false),
        "__radd__" => (Add, true),
        "__sub__" => (Sub, false),
        "__rsub__" => (Sub, true),
        "__mul__" => (Mult, false),
        "__rmul__" => (Mult, true),
        "__truediv__" => (Div, false),
        "__rtruediv__" => (Div, true),
        "__floordiv__" => (FloorDiv, false),
        "__rfloordiv__" => (FloorDiv, true),
        "__mod__" => (Mod, false),
        "__rmod__" => (Mod, true),
        "__pow__" => (Pow, false),
        "__rpow__" => (Pow, true),
        "__matmul__" => (MatMult, false),
        "__rmatmul__" => (MatMult, true),
        "__and__" => (BitAnd, false),
        "__rand__" => (BitAnd, true),
        "__or__" => (BitOr, false),
        "__ror__" => (BitOr, true),
        "__xor__" => (BitXor, false),
        "__rxor__" => (BitXor, true),
        "__lshift__" => (LShift, false),
        "__rlshift__" => (LShift, true),
        "__rshift__" => (RShift, false),
        "__rrshift__" => (RShift, true),
        // divmod uses a placeholder op; the caller builds the (q, r) tuple.
        "__divmod__" => (Add, false),
        "__rdivmod__" => (Add, true),
        _ => return None,
    };
    Some((op, reflected))
}

/// Integral part of a numeric value for `__floor__`/`__ceil__`/`__trunc__`.
/// Returns `None` for a non-numeric receiver.
fn numeric_integral(obj: &Value, method: &str) -> Option<Value> {
    use num_traits::ToPrimitive as _;
    match obj {
        Value::Int(_) | Value::BigInt(_) => Some(obj.clone()),
        Value::Bool(b) => Some(Value::Int(i64::from(*b))),
        Value::Float(f) => {
            let r = match method {
                "__floor__" => f.floor(),
                "__ceil__" => f.ceil(),
                _ => f.trunc(),
            };
            r.to_i64().map(Value::Int)
        }
        Value::Fraction(fr) => {
            let r = match method {
                "__floor__" => fr.floor(),
                "__ceil__" => fr.ceil(),
                _ => fr.trunc(),
            };
            Some(crate::value::int_from_bigint(r.to_integer()))
        }
        Value::Decimal(d, _) => {
            use bigdecimal::BigDecimal;
            let rounding = match method {
                "__floor__" => bigdecimal::RoundingMode::Floor,
                "__ceil__" => bigdecimal::RoundingMode::Ceiling,
                _ => bigdecimal::RoundingMode::Down,
            };
            let int_dec: BigDecimal = d.with_scale_round(0, rounding);
            let (bigint, _) = int_dec.as_bigint_and_exponent();
            Some(crate::value::int_from_bigint(bigint))
        }
        _ => None,
    }
}

pub(super) fn dispatch_method(
    obj: &mut Value,
    method: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> Result<MethodOutcome, EvalError> {
    // Reflective dunder calls on builtins map to their sync operator.
    if method.starts_with("__") {
        if let Some(outcome) = try_builtin_dunder(obj, method, args)? {
            return Ok(outcome);
        }
    }
    let Some(handler) = methods_handler_for(obj) else {
        debug_assert!(
            !crate::types::type_has_methods_table(obj),
            "type {} claims has_methods_table but has no handler",
            crate::types::type_name_of(obj)
        );
        return Err(InterpreterError::AttributeError(format!(
            "'{}' object has no attribute '{method}'",
            obj.type_name()
        ))
        .into());
    };
    handler(obj, method, args, kwargs)
}

/// Fetch the single required positional argument for a method, with a Python-
/// style `TypeError` naming the method when it is missing.
pub(crate) fn arg1<'a>(method: &str, args: &'a [Value]) -> Result<&'a Value, EvalError> {
    args.first().ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!("{method}() takes exactly 1 argument")))
    })
}
