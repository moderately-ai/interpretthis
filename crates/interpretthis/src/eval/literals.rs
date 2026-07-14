// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use rustpython_parser::ast::{self, Constant};

use crate::{
    error::EvalResult,
    eval::eval_expr,
    state::InterpreterState,
    tools::Tools,
    value::{Value, ValueKey, shared_list},
};

/// Convert a Python AST Constant to our Value type.
#[inline]
pub fn eval_constant(constant: &Constant) -> Value {
    match constant {
        // None and Ellipsis both evaluate to Value::None — Python's `...`
        // is a distinct object but we don't need a separate runtime value
        // for it yet, and None is the closest analogue.
        Constant::None | Constant::Ellipsis => Value::None,
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
        Constant::Complex { real, imag: _ } => {
            // No complex support yet — store as float (real part)
            Value::Float(*real)
        }
    }
}

/// Evaluate a list literal `[a, b, c]`.
pub async fn eval_list(
    state: &mut InterpreterState,
    node: &ast::ExprList,
    tools: &Tools,
) -> EvalResult {
    let mut items = Vec::with_capacity(node.elts.len());
    for elt in &node.elts {
        items.push(eval_expr(state, elt, tools).await?);
    }
    Ok(Value::List(shared_list(items)))
}

/// Evaluate a tuple literal `(a, b, c)`.
pub async fn eval_tuple(
    state: &mut InterpreterState,
    node: &ast::ExprTuple,
    tools: &Tools,
) -> EvalResult {
    let mut items = Vec::with_capacity(node.elts.len());
    for elt in &node.elts {
        items.push(eval_expr(state, elt, tools).await?);
    }
    Ok(Value::Tuple(items))
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
            // **dict unpacking
            let unpacked = eval_expr(state, value_expr, tools).await?;
            match unpacked {
                Value::Dict(d) => {
                    for (k, v) in d {
                        map.insert(k, v);
                    }
                }
                _ => {
                    return Err(crate::error::InterpreterError::TypeError(
                        "cannot unpack non-dict in dict literal".into(),
                    )
                    .into());
                }
            }
        }
    }
    Ok(Value::Dict(map))
}

/// Evaluate a set literal `{a, b, c}`.
pub async fn eval_set(
    state: &mut InterpreterState,
    node: &ast::ExprSet,
    tools: &Tools,
) -> EvalResult {
    let mut items: Vec<Value> = Vec::with_capacity(node.elts.len());
    // Side-table keyed dedup index for hashable values. Instance values
    // can't reduce to a ValueKey (their dedup is structural via __eq__
    // and stays O(N) per insert), but every builtin shape can — the
    // FxHashSet replaces the prior O(N²) double-iteration that
    // round-tripped each candidate against `value_to_key` of every
    // already-inserted item.
    // ValueKey is "mutable_key_type" only through Arc<Mutex<...>>
    // chains in LazyProxy / Function variants — those reject from
    // `value_to_key` (TypeError "unhashable"), so they never reach
    // this set in practice. The deep transitive interior-mutability
    // path is a clippy false positive against the safe-by-construction
    // shape here.
    #[expect(
        clippy::mutable_key_type,
        reason = "ValueKey only carries hashable variants in practice; \
                  unhashable Value variants are rejected upstream by value_to_key"
    )]
    let mut seen: rustc_hash::FxHashSet<crate::value::ValueKey> = rustc_hash::FxHashSet::default();
    for elt in &node.elts {
        let candidate = eval_expr(state, elt, tools).await?;
        let exists = if let Value::Instance(_) = &candidate {
            // Instance dedup via async `__eq__` (structural scan misses
            // custom equality such as case-insensitive wrappers).
            let mut found = false;
            for v in &items {
                if crate::eval::op::eq(state, v, &candidate, tools).await? {
                    found = true;
                    break;
                }
            }
            found
        } else {
            value_to_key(&candidate).is_ok_and(|ck| !seen.insert(ck))
        };
        if !exists {
            items.push(candidate);
        }
    }
    Ok(Value::Set(items))
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
        Value::Bool(b) => Ok(ValueKey::Bool(*b)),
        Value::Int(i) => Ok(ValueKey::Int(*i)),
        Value::BigInt(i) => Ok(ValueKey::BigInt((**i).clone())),
        Value::Float(f) => Ok(float_to_key(*f)),
        Value::String(s) => Ok(ValueKey::String(s.clone())),
        Value::Tuple(items) => {
            let keys: Result<Vec<ValueKey>, _> = items.iter().map(value_to_key).collect();
            Ok(ValueKey::Tuple(keys?))
        }
        _ => Err(crate::error::InterpreterError::TypeError(format!(
            "unhashable type: '{}'",
            val.type_name()
        ))
        .into()),
    }
}

/// Derive the hashable key for a float, folding exact integers into `Int`.
///
/// CPython unifies numeric keys: `hash(2.0) == hash(2)` and `{2: x}[2.0]` hits
/// the same slot. Folding a float with an exact `i64` value into `ValueKey::Int`
/// preserves that dict invariant (Python-equal values share one slot) across the
/// int/float boundary — the load-bearing correctness property, since a dict
/// holding two equal-but-distinct keys silently corrupts `in`/`len`/lookup.
///
/// The fold uses a round-trip guard (`as_int as f64 == f`): only values whose
/// `i64` conversion is exact are folded, so `1e30` and any non-integral float
/// keep their bit pattern. NaN/±inf are not finite, so they also keep bits and
/// match only an identical bit pattern (a NaN key thus never re-matches a freshly
/// computed NaN, mirroring CPython's identity-based NaN keys closely enough).
///
/// Known cosmetic deviation: a standalone integral-float key prints as the int
/// (`{2.0: x}` → `{2: x}`) because the stored key is `Int(2)`. CPython retains
/// the first-inserted key object and would print `2.0`. Preserving the equality
/// invariant is worth this display difference; full fidelity needs a separate
/// stored-key vs. lookup-key split, which the `IndexMap<ValueKey, _>` model does
/// not have.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    reason = "round-trip guarded: `Int(as_int)` is returned only when \
              `as_int as f64 == f` — an exact equality check is the point (an \
              epsilon comparison would mis-fold non-integral values), so the \
              truncating cast is exact and any precision loss falls through to \
              the bit-pattern key"
)]
fn float_to_key(f: f64) -> ValueKey {
    if f.is_finite() && f.fract() == 0.0 {
        let as_int = f as i64;
        if as_int as f64 == f {
            return ValueKey::Int(as_int);
        }
    }
    ValueKey::Float(f.to_bits())
}
