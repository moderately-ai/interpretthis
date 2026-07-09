// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Lvalue (assignment-target) resolution.
//!
//! A *place* is a root variable plus a chain of already-evaluated accessor steps
//! — the address of a mutable slot in the value tree. It backs every operation
//! that has to write *through* an expression rather than to a bare name:
//!
//!   * subscript / attribute assignment   — `d["a"]["x"] = v`, `obj.attr = v`
//!   * slice assignment                    — `lst[1:] = xs`
//!   * augmented assignment                — `d["a"]["x"] += 1`
//!   * in-place mutating method calls      — `groups[1].append(5)`
//!
//! The design resolves the place once (evaluating any index expressions) and
//! then navigates a *single* `&mut` borrow into `state.variables` to reach the
//! slot. Nothing is cloned: the previous implementation cloned the receiver and
//! then scanned every variable's `Debug` string to discover where to write the
//! mutation back — O(variables × value size) per call, and wrong for any slot
//! that was not a top-level variable. Navigation is O(path length); memory
//! accounting is an O(1) signed byte delta applied after the borrow ends.

use rustpython_parser::ast::Expr;

use crate::{
    error::{EvalError, InterpreterError},
    eval::{control_flow::iterate_value, eval_expr, literals::value_to_key},
    state::{InterpreterState, estimate_value_size},
    tools::Tools,
    value::{ExceptionValue, Value, ValueKey},
};

/// One accessor in a place path, with index/slice expressions already evaluated.
pub(crate) enum PlaceStep {
    /// `obj[key]` — `key` evaluated to a value.
    Index(Value),
    /// `obj[lower:upper:step]` — bounds evaluated. Only valid as the terminal
    /// step of a plain assignment; it is not a navigable intermediate slot.
    /// Boxed because the three optional bound `Value`s make it far larger than
    /// the other variants.
    Slice(Box<SliceSpec>),
    /// `obj.attr`.
    Attr(String),
}

/// Evaluated bounds of a slice target.
pub(crate) struct SliceSpec {
    pub lower: Option<Value>,
    pub upper: Option<Value>,
    pub step: Option<Value>,
}

/// A resolved assignment target: a root variable and the path to the slot.
pub(crate) struct Place {
    pub root: String,
    pub steps: Vec<PlaceStep>,
}

impl Place {
    /// True when every step can be navigated as a persistent mutable slot.
    /// A `Slice` step cannot (it denotes a fresh sub-sequence), so a receiver
    /// containing one is treated as a temporary by the method-call path —
    /// matching CPython, where `lst[1:].append(x)` mutates a throwaway list.
    pub(crate) fn is_navigable(&self) -> bool {
        self.steps.iter().all(|s| !matches!(s, PlaceStep::Slice(_)))
    }
}

/// Resolve an expression to a [`Place`], evaluating any index/slice
/// sub-expressions left-to-right. Returns `Ok(None)` when the expression is not
/// rooted in a bare variable (e.g. `f()[0]`, a literal) and therefore cannot
/// name a persistent slot.
pub(crate) fn eval_place<'a>(
    state: &'a mut InterpreterState,
    expr: &'a Expr,
    tools: &'a Tools,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Option<Place>, EvalError>> + Send + 'a>,
> {
    Box::pin(async move {
        match expr {
            Expr::Name(name_node) => {
                Ok(Some(Place { root: name_node.id.as_str().to_string(), steps: Vec::new() }))
            }
            Expr::Subscript(sub) => {
                let Some(mut place) = eval_place(state, &sub.value, tools).await? else {
                    return Ok(None);
                };
                if let Expr::Slice(slice) = sub.slice.as_ref() {
                    let lower = eval_opt(state, slice.lower.as_deref(), tools).await?;
                    let upper = eval_opt(state, slice.upper.as_deref(), tools).await?;
                    let step = eval_opt(state, slice.step.as_deref(), tools).await?;
                    place.steps.push(PlaceStep::Slice(Box::new(SliceSpec { lower, upper, step })));
                } else {
                    let key = eval_expr(state, &sub.slice, tools).await?;
                    place.steps.push(PlaceStep::Index(key));
                }
                Ok(Some(place))
            }
            Expr::Attribute(attr) => {
                crate::security::validator::validate_attribute(attr.attr.as_str())?;
                let Some(mut place) = eval_place(state, &attr.value, tools).await? else {
                    return Ok(None);
                };
                place.steps.push(PlaceStep::Attr(attr.attr.as_str().to_string()));
                Ok(Some(place))
            }
            _ => Ok(None),
        }
    })
}

/// Evaluate an optional slice-bound expression.
async fn eval_opt(
    state: &mut InterpreterState,
    expr: Option<&Expr>,
    tools: &Tools,
) -> Result<Option<Value>, EvalError> {
    match expr {
        Some(e) => Ok(Some(eval_expr(state, e, tools).await?)),
        None => Ok(None),
    }
}

/// Navigate `root` along `steps`, then invoke `f` with a mutable
/// reference to the final slot. The callback shape is the API that
/// shared-container types need (List wrapped in `Arc<Mutex<Vec>>`,
/// etc.): the lock guard is held only for the duration of `f`, so the
/// caller can mutate the slot in-place without lifetime gymnastics on
/// the returned borrow.
///
/// Walks the steps recursively, holding any intermediate
/// `parking_lot::MutexGuard`s on the stack so the lock guard's scope
/// covers `f`'s execution. Callers pass an `FnOnce(&mut Value) -> R`
/// and never see the locking machinery. Type erasure via
/// `Box<dyn FnOnce>` is used to make the recursion's chained-closure
/// type compile without monomorphisation blowup.
pub(crate) fn with_navigate_mut<R, F: FnOnce(&mut Value) -> R>(
    root: &mut Value,
    steps: &[PlaceStep],
    f: F,
) -> Result<R, EvalError> {
    let f_boxed: NavCallback<'_, R> = Box::new(move |v| Ok(f(v)));
    nav_recurse(root, steps, f_boxed)
}

/// Type-erased navigation callback: takes a `&mut Value` slot and
/// returns either a successful value of type `R` or an `EvalError`.
/// The boxed dyn FnOnce is what makes the recursion compile — without
/// it, the chained-closure type grows unboundedly with step depth.
type NavCallback<'a, R> = Box<dyn FnOnce(&mut Value) -> Result<R, EvalError> + 'a>;

fn nav_recurse<R>(
    cur: &mut Value,
    steps: &[PlaceStep],
    f: NavCallback<'_, R>,
) -> Result<R, EvalError> {
    let Some((head, tail)) = steps.split_first() else {
        return f(cur);
    };
    match head {
        PlaceStep::Index(key) => match cur {
            Value::List(items) => {
                let mut guard = items.lock();
                let idx = seq_index(guard.len(), key)?;
                nav_recurse(&mut guard[idx], tail, f)
            }
            Value::Dict(map) => {
                let k = value_to_key(key)?;
                let v = map
                    .get_mut(&k)
                    .ok_or_else(|| EvalError::Exception(ExceptionValue::key_error(&k)))?;
                nav_recurse(v, tail, f)
            }
            Value::DefaultDict(data) => {
                let k = value_to_key(key)?;
                // The aug-assign pre-touch synthesised the entry before
                // navigation; if it's still missing, the caller didn't
                // go through eval_aug_assign and the synth-on-miss
                // contract was bypassed. Raise KeyError to signal that.
                let v = data
                    .items
                    .get_mut(&k)
                    .ok_or_else(|| EvalError::Exception(ExceptionValue::key_error(&k)))?;
                nav_recurse(v, tail, f)
            }
            other => Err(InterpreterError::TypeError(format!(
                "'{}' object is not subscriptable",
                other.type_name()
            ))
            .into()),
        },
        PlaceStep::Attr(name) => match cur {
            Value::Dict(map) => {
                let v = map.get_mut(&ValueKey::String(name.as_str().into())).ok_or_else(
                    || -> EvalError {
                        InterpreterError::AttributeError(format!(
                            "'dict' object has no attribute '{name}'"
                        ))
                        .into()
                    },
                )?;
                nav_recurse(v, tail, f)
            }
            Value::Instance(inst) => {
                let class_name = inst.class_name.clone();
                let v = inst.fields.get_mut(name.as_str()).ok_or_else(|| -> EvalError {
                    InterpreterError::AttributeError(format!(
                        "'{class_name}' object has no attribute '{name}'"
                    ))
                    .into()
                })?;
                nav_recurse(v, tail, f)
            }
            other => Err(InterpreterError::AttributeError(format!(
                "'{}' object has no attribute '{name}'",
                other.type_name()
            ))
            .into()),
        },
        PlaceStep::Slice(_) => Err(InterpreterError::Runtime(
            "a slice cannot be used as an intermediate assignment target".into(),
        )
        .into()),
    }
}

/// Resolve a sequence index (supports `True`/`False` as 1/0 and negatives),
/// bounds-checked against `len`.
fn seq_index(len: usize, key: &Value) -> Result<usize, EvalError> {
    let raw = match key {
        Value::Int(i) => *i,
        Value::Bool(b) => i64::from(*b),
        other => {
            return Err(InterpreterError::TypeError(format!(
                "list indices must be integers, not '{}'",
                other.type_name()
            ))
            .into());
        }
    };
    let len_i = i64::try_from(len).map_err(|_| {
        EvalError::from(InterpreterError::Runtime("sequence length overflows i64".into()))
    })?;
    let idx = if raw < 0 { len_i + raw } else { raw };
    if idx < 0 || idx >= len_i {
        return Err(EvalError::Exception(ExceptionValue::index_error("list")));
    }
    usize::try_from(idx).map_err(|_| {
        EvalError::from(InterpreterError::Runtime("index overflow (internal invariant)".into()))
    })
}

/// Apply the terminal accessor of an assignment target, writing `value` into the
/// slot. Returns the signed change in the container's estimated heap size so the
/// caller can update the memory budget in O(1) without re-estimating the root.
pub(crate) fn assign_terminal(
    container: &mut Value,
    step: &PlaceStep,
    value: Value,
) -> Result<isize, EvalError> {
    match step {
        PlaceStep::Index(key) => set_index(container, key, value),
        PlaceStep::Attr(name) => set_attr(container, name, value),
        PlaceStep::Slice(spec) => set_slice(container, spec, value),
    }
}

/// `container[key] = value`.
///
/// Routes through `types::dispatch_setitem` so the per-type write logic
/// lives in one place; the dispatch returns the signed byte delta the
/// caller folds into the memory budget.
fn set_index(container: &mut Value, key: &Value, value: Value) -> Result<isize, EvalError> {
    crate::types::dispatch_setitem(container, key, value)
}

/// `container.attr = value` — Instance fields stay on the legacy path
/// here until B1 promotes `Value::Instance` to its own TypeObject with
/// a populated `set_attr_slot`. All other types (including Dict) route
/// through `types::dispatch_setattr`, which raises CPython's
/// `AttributeError("'<name>' object has no attribute '<name>'")` shape
/// for read-only types like list/tuple/str.
fn set_attr(container: &mut Value, name: &str, value: Value) -> Result<isize, EvalError> {
    if let Value::Instance(inst) = container {
        let new_size = estimate_value_size(&value);
        let delta = inst.fields.insert(name.to_string(), value).map_or_else(
            || to_isize(name.len() + new_size),
            |old| size_delta(estimate_value_size(&old), new_size),
        );
        return Ok(delta);
    }
    crate::types::dispatch_setattr(container, name, value)
}

/// `container[lower:upper:step] = iterable`. List is the only container
/// that supports slice assignment in CPython.
fn set_slice(container: &mut Value, spec: &SliceSpec, value: Value) -> Result<isize, EvalError> {
    let Value::List(items) = container else {
        return Err(InterpreterError::TypeError(format!(
            "'{}' object does not support slice assignment",
            container.type_name()
        ))
        .into());
    };
    let new_items = iterate_value(&value)?;
    let stride = resolve_step(spec.step.as_ref())?;
    let mut guard = items.lock();
    let len = i64::try_from(guard.len()).map_err(|_| {
        EvalError::from(InterpreterError::Runtime("sequence length overflows i64".into()))
    })?;

    if stride == 1 {
        let start = clamp_index(resolve_bound(spec.lower.as_ref(), 0)?, len);
        let stop = clamp_index(resolve_bound(spec.upper.as_ref(), len)?, len).max(start);
        let lo = to_index(start)?;
        let hi = to_index(stop)?;
        let removed_size: usize = guard[lo..hi].iter().map(estimate_value_size).sum();
        let added_size: usize = new_items.iter().map(estimate_value_size).sum();
        guard.splice(lo..hi, new_items);
        return Ok(size_delta(removed_size, added_size));
    }

    // Extended slice: indices are fixed, so the RHS length must match exactly.
    let indices = extended_indices(len, spec, stride)?;
    if indices.len() != new_items.len() {
        return Err(InterpreterError::ValueError(format!(
            "attempt to assign sequence of size {} to extended slice of size {}",
            new_items.len(),
            indices.len()
        ))
        .into());
    }
    let mut delta = 0isize;
    for (idx, val) in indices.into_iter().zip(new_items) {
        delta = delta.saturating_add(size_delta(
            estimate_value_size(&guard[idx]),
            estimate_value_size(&val),
        ));
        guard[idx] = val;
    }
    drop(guard);
    Ok(delta)
}

/// Collect the concrete indices selected by an extended (step != 1) slice.
fn extended_indices(len: i64, spec: &SliceSpec, stride: i64) -> Result<Vec<usize>, EvalError> {
    let mut indices = Vec::new();
    if stride > 0 {
        let start = clamp_index(resolve_bound(spec.lower.as_ref(), 0)?, len);
        let stop = clamp_index(resolve_bound(spec.upper.as_ref(), len)?, len);
        let mut i = start;
        while i < stop {
            indices.push(to_index(i)?);
            i += stride;
        }
    } else {
        let start = clamp_index_neg(resolve_bound(spec.lower.as_ref(), len - 1)?, len);
        let stop = clamp_index_neg(resolve_bound(spec.upper.as_ref(), -(len + 1))?, len);
        let mut i = start;
        while i > stop {
            indices.push(to_index(i)?);
            i += stride;
        }
    }
    Ok(indices)
}

/// Resolve a slice step value to a non-zero i64 stride (default 1).
fn resolve_step(step: Option<&Value>) -> Result<i64, EvalError> {
    match step {
        Some(Value::Int(s)) if *s != 0 => Ok(*s),
        Some(Value::Int(_)) => {
            Err(InterpreterError::ValueError("slice step cannot be zero".into()).into())
        }
        // Absent, `None`, or a bool step all mean stride 1.
        None | Some(Value::None | Value::Bool(_)) => Ok(1),
        Some(other) => Err(InterpreterError::TypeError(format!(
            "slice indices must be integers or None, not '{}'",
            other.type_name()
        ))
        .into()),
    }
}

/// Resolve a slice bound value to an i64, using `default` for absent/`None`.
fn resolve_bound(val: Option<&Value>, default: i64) -> Result<i64, EvalError> {
    match val {
        None | Some(Value::None) => Ok(default),
        Some(Value::Int(i)) => Ok(*i),
        Some(Value::Bool(b)) => Ok(i64::from(*b)),
        Some(other) => Err(InterpreterError::TypeError(format!(
            "slice indices must be integers or None, not '{}'",
            other.type_name()
        ))
        .into()),
    }
}

/// Clamp a positive-step slice index into `[0, len]`.
fn clamp_index(idx: i64, len: i64) -> i64 {
    let adjusted = if idx < 0 { idx + len } else { idx };
    adjusted.clamp(0, len)
}

/// Clamp a negative-step slice index into `[-1, len - 1]`.
fn clamp_index_neg(idx: i64, len: i64) -> i64 {
    let adjusted = if idx < 0 { idx + len } else { idx };
    adjusted.clamp(-1, len - 1)
}

/// Convert a non-negative slice index into a `usize`. Callers keep the value in
/// range, so the conversion cannot fail in practice.
fn to_index(i: i64) -> Result<usize, EvalError> {
    usize::try_from(i).map_err(|_| {
        EvalError::from(InterpreterError::Runtime(
            "slice index overflow (internal invariant)".into(),
        ))
    })
}

/// Signed `new - old` byte delta, saturating rather than wrapping.
pub(crate) fn size_delta(old: usize, new: usize) -> isize {
    to_isize(new).saturating_sub(to_isize(old))
}

/// Convert a byte count into `isize`, saturating at `isize::MAX`. Sizes are
/// bounded by the memory limit (well under `isize::MAX`), so this never clamps
/// in practice — it is the lint-clean conversion at the boundary.
pub(crate) fn to_isize(n: usize) -> isize {
    isize::try_from(n).unwrap_or(isize::MAX)
}

/// Apply a signed memory delta to the interpreter's budget, enforcing the limit
/// on growth. Called after the mutable borrow into `state` has ended.
pub(crate) fn apply_mem_delta(state: &mut InterpreterState, delta: isize) -> Result<(), EvalError> {
    if delta >= 0 {
        state.track_allocation(usize::try_from(delta).unwrap_or(0)).map_err(EvalError::Interpreter)
    } else {
        state.release_allocation(usize::try_from(-delta).unwrap_or(0));
        Ok(())
    }
}
