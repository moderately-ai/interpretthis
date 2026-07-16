// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use rustpython_parser::ast::{self, Constant, Expr};

use crate::{
    error::{EvalError, EvalResult},
    eval::eval_expr,
    state::InterpreterState,
    tools::Tools,
    value::{Value, ValueKey, shared_list},
};

/// Convert a Python AST Constant to our Value type.
#[inline]
pub fn eval_constant(constant: &Constant) -> Value {
    match constant {
        Constant::None => Value::None,
        // `...` is the distinct Ellipsis singleton, not None.
        Constant::Ellipsis => Value::Ellipsis,
        Constant::Bool(b) => Value::Bool(*b),
        Constant::Int(i) => {
            // The parser is built with its `num-bigint` feature, so the AST's
            // integer literals are already `num_bigint::BigInt` — the same type
            // `Value::BigInt` holds. This used to round-trip through a decimal
            // string because the AST was malachite-backed (see the note on
            // `rustpython-parser` in the workspace manifest); it is now a move.
            //
            // `int_from_bigint` keeps anything that fits on the compact i64 path.
            crate::value::int_from_bigint(i.clone())
        }
        Constant::Float(f) => Value::Float(*f),
        Constant::Str(s) => Value::String(s.as_str().into()),
        Constant::Bytes(b) => Value::Bytes(b.clone()),
        Constant::Tuple(items) => Value::Tuple(items.iter().map(eval_constant).collect()),
        Constant::Complex { real, imag } => {
            Value::Complex(Box::new(num_complex::Complex64::new(*real, *imag)))
        }
    }
}

/// Evaluate a list literal `[a, b, c]`.
/// Evaluate the elements of a list/tuple/set display, splatting any `*expr`
/// element into its iterated items (`[*a, b, *c]`). A non-starred element
/// contributes a single value.
async fn eval_display_elements(
    state: &mut InterpreterState,
    elts: &[Expr],
    tools: &Tools,
) -> Result<Vec<Value>, EvalError> {
    let mut items = Vec::with_capacity(elts.len());
    for elt in elts {
        if let Expr::Starred(star) = elt {
            let value = eval_expr(state, &star.value, tools).await?;
            // Use the async iterator protocol so a user-class instance's
            // `__iter__`/`__next__` is honoured (the sync `iterate_value`
            // only handles builtin containers).
            items.extend(crate::eval::op::iter(state, &value, tools).await?);
        } else {
            items.push(eval_expr(state, elt, tools).await?);
        }
    }
    Ok(items)
}

pub async fn eval_list(
    state: &mut InterpreterState,
    node: &ast::ExprList,
    tools: &Tools,
) -> EvalResult {
    Ok(Value::List(shared_list(eval_display_elements(state, &node.elts, tools).await?)))
}

/// Evaluate a tuple literal `(a, b, c)`.
pub async fn eval_tuple(
    state: &mut InterpreterState,
    node: &ast::ExprTuple,
    tools: &Tools,
) -> EvalResult {
    Ok(Value::Tuple(eval_display_elements(state, &node.elts, tools).await?))
}

/// Evaluate a dict literal `{k: v, ...}`. Supports `**dict` unpacking (key=None).
pub async fn eval_dict(
    state: &mut InterpreterState,
    node: &ast::ExprDict,
    tools: &Tools,
) -> EvalResult {
    let mut map = indexmap::IndexMap::new();
    for (key_opt, value_expr) in node.keys.iter().zip(node.values.iter()) {
        if let Some(key_expr) = key_opt {
            let key = eval_expr(state, key_expr, tools).await?;
            let val = eval_expr(state, value_expr, tools).await?;
            if matches!(key, Value::Instance(_)) {
                // Instance keys: hash + `__eq__` replace (not structural IndexMap Eq).
                crate::eval::op::dict_insert_instance_key_pub(state, &mut map, &key, val, tools)
                    .await?;
            } else {
                map.insert(crate::eval::op::key(state, &key, tools).await?, val);
            }
        } else {
            // **dict unpacking (dict or OrderedDict)
            let unpacked = eval_expr(state, value_expr, tools).await?;
            if let Some(d) = unpacked.as_dict() {
                for (k, v) in d.lock().iter() {
                    map.insert(k.clone(), v.clone());
                }
            } else {
                return Err(crate::error::InterpreterError::TypeError(
                    "cannot unpack non-dict in dict literal".into(),
                )
                .into());
            }
        }
    }
    Ok(Value::Dict(crate::value::shared_dict(map)))
}

/// Build a `Value::Set` from already-evaluated candidates, deduplicating and
/// rejecting unhashable elements.
///
/// The single source of set construction — shared by the set literal, the
/// `set()` builtin, and set comprehensions, so all three agree on two things
/// they previously got wrong when they open-coded a `value_to_key(x).ok()`
/// dedup:
/// - an unhashable element (list, dict, set) raises `TypeError: unhashable
///   type`, rather than being silently included;
/// - instances dedup by their `__eq__` (structural / custom), rather than all
///   collapsing to one because `value_to_key` returns `None` for every instance.
pub(crate) async fn build_set(
    state: &mut InterpreterState,
    candidates: Vec<Value>,
    // A constant set/frozenset literal (`{'a','b'}`, all-constant elements) is
    // folded by CPython's compiler into a `frozenset` constant with a distinct
    // iteration order; `constant` selects that fold over incremental order.
    constant: bool,
    tools: &Tools,
) -> EvalResult {
    let mut items: Vec<Value> = Vec::with_capacity(candidates.len());
    // Keyed dedup index for hashable values. ValueKey only carries hashable
    // variants (unhashable Values are rejected below by value_to_key), so the
    // interior-mutability clippy lint is a false positive here.
    #[expect(
        clippy::mutable_key_type,
        reason = "ValueKey only carries hashable variants; unhashable Values are rejected by \
                  value_to_key before reaching the set"
    )]
    let mut seen: rustc_hash::FxHashSet<crate::value::ValueKey> = rustc_hash::FxHashSet::default();
    for candidate in candidates {
        let exists = if let Value::Instance(_) = &candidate {
            // Validate hashability first (raises `TypeError: unhashable type` for
            // a class with `__hash__ = None`, `__eq__` without `__hash__`, or a
            // default dataclass) — a set member must be hashable, and instance
            // dedup otherwise skips the hash check that non-instances get.
            crate::eval::op::hash(state, &candidate, tools).await?;
            // Instance dedup via async `__eq__` (structural scan misses custom
            // equality such as case-insensitive wrappers).
            let mut found = false;
            for v in &items {
                if crate::eval::op::eq(state, v, &candidate, tools).await? {
                    found = true;
                    break;
                }
            }
            found
        } else {
            // A non-instance candidate that is not hashable (list, dict, set)
            // raises `TypeError: unhashable type`.
            let ck = value_to_key(&candidate)?;
            !seen.insert(ck)
        };
        if !exists {
            items.push(candidate);
        }
    }
    let body = if constant {
        crate::pyset::SetBody::from_constant_literal(items)
    } else {
        crate::pyset::SetBody::from_items(items)
    };
    Ok(Value::Set(crate::value::shared_set(body)))
}

/// Evaluate a set literal `{a, b, c}`.
pub async fn eval_set(
    state: &mut InterpreterState,
    node: &ast::ExprSet,
    tools: &Tools,
) -> EvalResult {
    let candidates = eval_display_elements(state, &node.elts, tools).await?;
    // CPython's compiler constant-folds an all-constant set literal.
    let constant =
        !node.elts.is_empty() && node.elts.iter().all(|e| matches!(e, ast::Expr::Constant(_)));
    build_set(state, candidates, constant, tools).await
}

/// Convert a Value to a hashable dict key.
///
/// Floats are hashable in CPython: `{1.5: x}`, `hash(1.5)`, and `set([1.5])`
/// all work. The float arm folds integral floats into `Int(...)` so
/// `{2.0: x}[2]` hits the same slot as `{2: x}[2.0]`.
///
/// Bools are preserved as `ValueKey::Bool` so downstream consumers (e.g.
/// `json.dumps` emitting `"true"/"false"` per the JSON spec) can still see
/// the original type. The numeric-equivalence with `Int(0|1)` is enforced
/// by custom `PartialEq`/`Eq`/`Hash` impls on `ValueKey` (see `value.rs`)
/// so `{True: x}[1]` resolves to `x` — closes the user-listed bool↔int
/// dict-key gap without destroying the type info CPython keeps on the
/// stored key object.
#[inline]
pub fn value_to_key(val: &Value) -> Result<ValueKey, crate::error::EvalError> {
    match val {
        Value::None => Ok(ValueKey::None),
        Value::Ellipsis => Ok(ValueKey::Ellipsis),
        Value::Bool(b) => Ok(ValueKey::Bool(*b)),
        Value::Int(i) => Ok(ValueKey::Int(*i)),
        Value::BigInt(i) => Ok(ValueKey::BigInt((**i).clone())),
        Value::Float(f) => Ok(float_to_key(*f)),
        // A real complex (`imag == 0`) folds to the float/int key so it shares a
        // slot with equal ints/floats (`{1, 1+0j}` -> one entry). `+0.0`
        // normalises `-0.0` so signed zeros key alike.
        Value::Complex(c) if c.im == 0.0 => Ok(float_to_key(c.re)),
        Value::Complex(c) => Ok(ValueKey::Complex((c.re + 0.0).to_bits(), (c.im + 0.0).to_bits())),
        Value::String(s) => Ok(ValueKey::String(s.clone())),
        Value::Tuple(items) => {
            let keys: Result<Vec<ValueKey>, _> = items.iter().map(value_to_key).collect();
            Ok(ValueKey::Tuple(keys?))
        }
        // A frozenset is hashable; its elements are already hashable.
        Value::Frozenset(body) => {
            let keys: Result<Vec<ValueKey>, _> =
                body.iter_ordered().iter().map(value_to_key).collect();
            Ok(ValueKey::Frozenset(keys?))
        }
        // Temporal types are hashable in CPython (usable as dict keys / set
        // members). The key retains the original fields; equality/hash
        // normalise aware datetimes to their UTC instant (see ValueKey).
        Value::Date(d) => Ok(ValueKey::Date(*d)),
        Value::Time(t) => Ok(ValueKey::Time(*t)),
        Value::TimeDelta(m) => Ok(ValueKey::TimeDelta(*m)),
        Value::DateTime { dt, tz_offset_secs } => {
            Ok(ValueKey::DateTime { dt: *dt, tz_offset_secs: *tz_offset_secs })
        }
        // Decimal / Fraction are hashable in CPython. An integer-valued one
        // folds to the int key so `Decimal('2')` / `Fraction(4, 2)` shares a
        // dict/set slot with `2` (CPython unifies numeric hashes); a
        // non-integral one keeps a dedicated, value-based key.
        Value::Decimal(d, _) => {
            if d.normalized().fractional_digit_count() <= 0 {
                let n = d.with_scale(0).as_bigint_and_exponent().0;
                Ok(bigint_to_key(n))
            } else {
                Ok(ValueKey::Decimal(Box::new((**d).clone())))
            }
        }
        Value::Fraction(fr) => {
            if fr.is_integer() {
                Ok(bigint_to_key(fr.to_integer()))
            } else {
                Ok(ValueKey::Fraction(Box::new((**fr).clone())))
            }
        }
        // Enum members are hashable in CPython. An IntEnum / IntFlag / StrEnum
        // member hashes and keys as its underlying int / str (so `hash(P.HIGH)
        // == hash(10)` and `{P.HIGH: 1}[10]` hits the same slot). A plain Enum /
        // Flag hashes by member identity (class + member name), matching
        // CPython's default object hash while staying deterministic across runs.
        Value::EnumMember { value: inner, kind, class_name, member_name } => match kind {
            crate::value::EnumKind::Int
            | crate::value::EnumKind::IntFlag
            | crate::value::EnumKind::Str => value_to_key(inner),
            crate::value::EnumKind::Plain | crate::value::EnumKind::Flag => {
                use std::hash::{Hash as _, Hasher as _};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                class_name.hash(&mut hasher);
                member_name.hash(&mut hasher);
                #[expect(
                    clippy::cast_possible_wrap,
                    reason = "hash bits reinterpreted as i64 — CPython hashes are also signed"
                )]
                Ok(ValueKey::Instance {
                    hash: hasher.finish() as i64,
                    value: Box::new(val.clone()),
                })
            }
        },
        // Functions and lambdas are hashable in CPython (by object identity),
        // so they can be dict keys / set members. Key on the `Arc` pointer:
        // distinct function objects get distinct keys, the same object (shared
        // `Arc`) keys alike. The hash is identity-based (not CPython's exact
        // address hash, which is non-deterministic there too).
        Value::Function(fd) => Ok(ValueKey::Instance {
            hash: std::sync::Arc::as_ptr(fd) as *const () as usize as i64,
            value: Box::new(val.clone()),
        }),
        Value::Lambda(ld) => Ok(ValueKey::Instance {
            hash: std::sync::Arc::as_ptr(ld) as *const () as usize as i64,
            value: Box::new(val.clone()),
        }),
        _ => Err(crate::error::InterpreterError::TypeError(format!(
            "unhashable type: '{}'",
            val.type_name()
        ))
        .into()),
    }
}

/// Derive the hashable key for a float. The float keeps its own `Float` key
/// (retaining its `2.0` repr), but an integral float is made cross-equal to the
/// matching `Int`/`BigInt` key via `ValueKey`'s hand-written `PartialEq`/`Hash`
/// (`hash(2.0) == hash(2)` and `{2: x}[2.0]` hit the same slot). So `{2.0: x}`
/// prints `{2.0: x}` (CPython fidelity) while `{2, 2.0}` still dedups.
fn float_to_key(f: f64) -> ValueKey {
    ValueKey::Float(f.to_bits())
}

/// Fold a `BigInt` into the narrowest integer key so it collides with an equal
/// `Int`/`Bool` key of the same magnitude (the `NUMERIC_TAG` hash bucket).
fn bigint_to_key(n: num_bigint::BigInt) -> ValueKey {
    match i64::try_from(&n) {
        Ok(i) => ValueKey::Int(i),
        Err(_) => ValueKey::BigInt(n),
    }
}
