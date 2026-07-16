// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use rustpython_parser::ast::{self, Expr};

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::eval_expr,
    state::InterpreterState,
    tools::Tools,
    value::Value,
};

/// Evaluate a delete statement.
pub async fn eval_delete(
    state: &mut InterpreterState,
    node: &ast::StmtDelete,
    tools: &Tools,
) -> EvalResult {
    for target in &node.targets {
        delete_target(state, target, tools).await?;
    }
    Ok(Value::None)
}

/// Delete a single target.
async fn delete_target(
    state: &mut InterpreterState,
    target: &Expr,
    tools: &Tools,
) -> Result<(), EvalError> {
    match target {
        Expr::Name(name_node) => {
            let name = name_node.id.as_str();

            // Block deleting tools
            if tools.contains_key(name) {
                return Err(InterpreterError::Security(format!(
                    "cannot delete static tool '{name}'"
                ))
                .into());
            }

            // Delete the variable
            state.delete_variable(name).map_err(EvalError::Interpreter)
        }
        // `del path.attr` — property deleter / __delattr__ when the base is a
        // bare name; otherwise place-navigate to the parent and drop the field
        // (covers `del a.b.c` and `del items[0].x` with shared instance fields).
        Expr::Attribute(attr_node) => {
            let attr_name = attr_node.attr.as_str().to_string();
            crate::security::validator::validate_attribute(&attr_name)?;

            // Fast path: bare `del name.attr` keeps property / __delattr__ hooks.
            if let Expr::Name(name_node) = attr_node.value.as_ref() {
                let obj_name = name_node.id.as_str().to_string();
                let obj = state.variables.get(&obj_name).cloned().ok_or_else(|| {
                    EvalError::from(InterpreterError::name_not_defined(&obj_name))
                })?;
                if let Value::Instance(inst) = &obj {
                    let class_name = inst.class_name.clone();
                    if let Some(desc) = crate::eval::classes::lookup_class_attr_instance(
                        state,
                        &class_name,
                        &attr_name,
                    ) {
                        if let Some((_, del_method)) = crate::eval::classes::lookup_method_in_mro(
                            state,
                            &desc.class_name,
                            "__delete__",
                        ) {
                            let call = crate::eval::functions::CallArgs {
                                positional: std::slice::from_ref(&obj),
                                keyword: &indexmap::IndexMap::new(),
                            };
                            let _ = crate::eval::classes::call_method(
                                state,
                                &del_method,
                                Value::Instance(desc),
                                call,
                                tools,
                            )
                            .await?;
                            return Ok(());
                        }
                    }
                    if let Some(prop) =
                        crate::eval::classes::lookup_property(state, &class_name, &attr_name)
                    {
                        let Some(deleter) = prop.deleter else {
                            return Err(InterpreterError::AttributeError(format!(
                                "property '{attr_name}' of '{class_name}' object has no deleter"
                            ))
                            .into());
                        };
                        let configured = crate::eval::classes::invoke_property_deleter(
                            state, &deleter, obj, tools,
                        )
                        .await?;
                        state
                            .set_variable(&obj_name, configured)
                            .map_err(EvalError::Interpreter)?;
                        return Ok(());
                    }
                    if let Some((_, method)) = crate::eval::classes::lookup_method_in_mro(
                        state,
                        &class_name,
                        "__delattr__",
                    ) {
                        let name_arg = Value::String(attr_name.as_str().into());
                        let call = crate::eval::functions::CallArgs {
                            positional: std::slice::from_ref(&name_arg),
                            keyword: &indexmap::IndexMap::new(),
                        };
                        let (_returned, updated_self) =
                            crate::eval::classes::call_method(state, &method, obj, call, tools)
                                .await?;
                        state
                            .set_variable(&obj_name, updated_self)
                            .map_err(EvalError::Interpreter)?;
                        return Ok(());
                    }
                    if inst.fields.lock().remove(&attr_name).is_some() {
                        return Ok(());
                    }
                    return Err(InterpreterError::AttributeError(format!(
                        "'{class_name}' object has no attribute '{attr_name}'"
                    ))
                    .into());
                }
            }

            // Complex path: navigate place to parent, delete attribute on Instance.
            let place = crate::eval::place::eval_place(state, target, tools)
                .await?
                .ok_or_else(|| {
                    EvalError::from(InterpreterError::Runtime(
                        "complex delete attribute target not supported (see CONFORMANCE.md#unsupported-language-features)".into(),
                    ))
                })?;
            if !place.is_navigable() {
                return Err(InterpreterError::Runtime(
                    "complex delete attribute target not supported (see CONFORMANCE.md#unsupported-language-features)".into(),
                )
                .into());
            }
            let Some((crate::eval::place::PlaceStep::Attr(name), prefix)) =
                place.steps.split_last()
            else {
                return Err(InterpreterError::Runtime(
                    "complex delete attribute target not supported (see CONFORMANCE.md#unsupported-language-features)".into(),
                )
                .into());
            };
            let root = state
                .variables
                .get_mut(&place.root)
                .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&place.root)))?;
            crate::eval::place::with_navigate_mut(root, prefix, |parent| {
                let Value::Instance(inst) = parent else {
                    return Err(EvalError::from(InterpreterError::AttributeError(format!(
                        "'{}' object attribute deletion not supported",
                        parent.type_name()
                    ))));
                };
                if inst.fields.lock().remove(name).is_none() {
                    return Err(EvalError::from(InterpreterError::AttributeError(format!(
                        "'{}' object has no attribute '{name}'",
                        inst.class_name
                    ))));
                }
                Ok(())
            })?
        }
        Expr::Subscript(sub_node) => {
            // Slice deletion is its own path — keeps the Name-only fast
            // path for `del lst[1:3]` and similar list-slice idioms.
            if let Expr::Slice(slice_node) = sub_node.slice.as_ref() {
                if let Expr::Name(name_node) = sub_node.value.as_ref() {
                    // A user-class instance deletes a slice via
                    // `__delitem__(slice(...))`, not the builtin slice deleter.
                    if matches!(
                        state.variables.get(name_node.id.as_str()),
                        Some(Value::Instance(_))
                    ) {
                        let start = match &slice_node.lower {
                            Some(e) => {
                                let v = eval_expr(state, e, tools).await?;
                                crate::eval::op::coerce_index(state, v, tools).await?
                            }
                            None => Value::None,
                        };
                        let stop = match &slice_node.upper {
                            Some(e) => {
                                let v = eval_expr(state, e, tools).await?;
                                crate::eval::op::coerce_index(state, v, tools).await?
                            }
                            None => Value::None,
                        };
                        let step = match &slice_node.step {
                            Some(e) => {
                                let v = eval_expr(state, e, tools).await?;
                                crate::eval::op::coerce_index(state, v, tools).await?
                            }
                            None => Value::None,
                        };
                        let slice_val =
                            Value::Slice(Box::new(crate::value::SliceValue { start, stop, step }));
                        let receiver = eval_expr(state, sub_node.value.as_ref(), tools).await?;
                        if let Some(updated_self) =
                            crate::eval::op::delitem(state, &receiver, &slice_val, tools).await?
                        {
                            return writeback_receiver(
                                state,
                                sub_node.value.as_ref(),
                                updated_self,
                            );
                        }
                    }
                    return delete_slice(state, name_node.id.as_str(), slice_node, tools).await;
                }
                return Err(InterpreterError::Runtime(
                    "slice deletion supported only on bare-name targets".into(),
                )
                .into());
            }

            // Evaluate the receiver and key. User-class `__delitem__`
            // dispatches via `op::delitem` and writes back the
            // post-call self. Builtin containers fall through to the
            // place machinery so nested receivers (`del obj.attr[k]`)
            // mutate the owning slot in place.
            let receiver = eval_expr(state, &sub_node.value, tools).await?;
            let index = eval_expr(state, &sub_node.slice, tools).await?;

            if let Some(updated_self) =
                crate::eval::op::delitem(state, &receiver, &index, tools).await?
            {
                return writeback_receiver(state, sub_node.value.as_ref(), updated_self);
            }

            // Builtin container path. Bare-Name receiver uses a
            // direct `variables.get_mut` to skip place-eval cost; any
            // more complex expression navigates via place machinery
            // to find the mutable slot.
            if let Expr::Name(name_node) = sub_node.value.as_ref() {
                let name = name_node.id.as_str();
                let delta: Result<isize, EvalError> = {
                    let obj = state.variables.get_mut(name).ok_or_else(|| {
                        EvalError::Interpreter(InterpreterError::name_not_defined(name))
                    })?;
                    crate::types::dispatch_delitem(obj, &index)
                };
                let freed = delta?.unsigned_abs();
                state.release_allocation(freed);
                return Ok(());
            }

            let place_opt = crate::eval::place::eval_place(state, &sub_node.value, tools).await?;
            let place = place_opt.ok_or_else(|| {
                EvalError::from(InterpreterError::Runtime(
                    "cannot resolve delete-subscript receiver".into(),
                ))
            })?;
            let delta: Result<isize, EvalError> = {
                let root = state.variables.get_mut(&place.root).ok_or_else(|| {
                    EvalError::Interpreter(InterpreterError::name_not_defined(&place.root))
                })?;
                crate::eval::place::with_navigate_mut(root, &place.steps, |container| {
                    crate::types::dispatch_delitem(container, &index)
                })?
            };
            let freed = delta?.unsigned_abs();
            state.release_allocation(freed);
            Ok(())
        }
        Expr::Tuple(tuple_node) => {
            for elt in &tuple_node.elts {
                Box::pin(delete_target(state, elt, tools)).await?;
            }
            Ok(())
        }
        Expr::List(list_node) => {
            for elt in &list_node.elts {
                Box::pin(delete_target(state, elt, tools)).await?;
            }
            Ok(())
        }
        _ => Err(InterpreterError::Runtime(format!(
            "cannot delete target: {:?}",
            std::mem::discriminant(target)
        ))
        .into()),
    }
}

/// Write the post-call self back through the receiver expression so
/// `del obj[k]` / `obj[k] = v` mutations visible to the caller's
/// binding propagate, whether the receiver is a bare Name or an
/// Attribute chain (`self.data[k]`). For deeper Subscript receivers
/// we currently fall back to discarding the mutation — mirror what
/// the assignment path does until the place machinery uniformly
/// owns the writeback for both surfaces.
fn writeback_receiver(
    state: &mut InterpreterState,
    receiver_expr: &Expr,
    updated_self: Value,
) -> Result<(), EvalError> {
    match receiver_expr {
        Expr::Name(name_node) => {
            state.set_variable(name_node.id.as_str(), updated_self).map_err(EvalError::Interpreter)
        }
        Expr::Attribute(attr_node) => {
            if let Expr::Name(name_node) = attr_node.value.as_ref() {
                let owner_name = name_node.id.as_str().to_string();
                let attr_name = attr_node.attr.as_str().to_string();
                if let Some(Value::Instance(inst)) = state.variables.get(&owner_name).cloned() {
                    inst.fields.lock().insert(attr_name, updated_self);
                    return state
                        .set_variable(&owner_name, Value::Instance(inst))
                        .map_err(EvalError::Interpreter);
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Delete a slice from a list (del lst[start:stop]).
async fn delete_slice(
    state: &mut InterpreterState,
    var_name: &str,
    slice_node: &ast::ExprSlice,
    tools: &Tools,
) -> Result<(), EvalError> {
    let start = if let Some(ref expr) = slice_node.lower {
        Some(eval_expr(state, expr, tools).await?)
    } else {
        None
    };
    let stop = if let Some(ref expr) = slice_node.upper {
        Some(eval_expr(state, expr, tools).await?)
    } else {
        None
    };
    let step = if let Some(ref expr) = slice_node.step {
        Some(eval_expr(state, expr, tools).await?)
    } else {
        None
    };
    // Resolve the step: absent/None -> 1; must be a non-zero integer.
    let step = match step {
        None | Some(Value::None) => 1,
        Some(Value::Int(n)) => n,
        Some(Value::Bool(b)) => i64::from(b),
        Some(_) => {
            return Err(InterpreterError::TypeError(
                "slice indices must be integers or None or have an __index__ method".to_string(),
            )
            .into());
        }
    };
    if step == 0 {
        return Err(InterpreterError::ValueError("slice step cannot be zero".into()).into());
    }

    let released: Result<usize, EvalError> = {
        let obj = state
            .variables
            .get_mut(var_name)
            .ok_or_else(|| EvalError::Interpreter(InterpreterError::name_not_defined(var_name)))?;

        if let Value::List(items) = obj {
            let mut guard = items.lock();
            let items_len = guard.len();
            let len = i64::try_from(items_len).map_err(|_| {
                InterpreterError::Runtime("list length overflows i64 for slicing".into())
            })?;

            // Clamp a Python-style slice bound (negative → relative from end,
            // out-of-range → saturated) to a valid `usize` position. Non-int
            // bounds fall through to `default`, matching the prior behavior.
            let clamp = |val: &Option<Value>, default: usize| -> Result<usize, EvalError> {
                match val {
                    Some(Value::Int(i)) => {
                        let idx = if *i < 0 { (len + *i).max(0) } else { *i };
                        let clamped = idx.min(len);
                        // clamped >= 0 && clamped <= len, len derived from usize.
                        usize::try_from(clamped).map_err(|_| {
                            InterpreterError::Runtime(
                                "slice index overflow (internal invariant)".into(),
                            )
                            .into()
                        })
                    }
                    _ => Ok(default),
                }
            };

            let result = if step == 1 {
                // Contiguous case: one drain.
                let s = clamp(&start, 0)?;
                let e = clamp(&stop, items_len)?;
                if s < e && s < items_len {
                    let end = e.min(items_len);
                    let drained_size: usize =
                        guard[s..end].iter().map(crate::state::estimate_value_size).sum();
                    guard.drain(s..end);
                    Ok(drained_size)
                } else {
                    Ok(0)
                }
            } else {
                // Extended slice: delete the strided positions (`del a[::2]`).
                // Reuse the read-path index generation, then remove each index
                // in descending order so earlier removals don't shift the rest.
                let indices = strided_indices(start.as_ref(), stop.as_ref(), step, len);
                let mut freed = 0usize;
                for &i in &indices {
                    if i < guard.len() {
                        freed += crate::state::estimate_value_size(&guard[i]);
                        guard.remove(i);
                    }
                }
                Ok(freed)
            };
            drop(guard);
            result
        } else {
            Err(EvalError::Interpreter(InterpreterError::TypeError(format!(
                "'{}' object does not support item deletion",
                obj.type_name()
            ))))
        }
    };
    let size = released?;
    if size > 0 {
        state.release_allocation(size);
    }
    Ok(())
}

/// The list positions selected by an extended slice `start:stop:step` (step != 1),
/// returned in descending order so a caller can `remove` each without shifting
/// the rest. Reuses the read-path clamp semantics so `del a[::2]` deletes the
/// same elements that `a[::2]` would read.
pub(crate) fn strided_indices(
    lower: Option<&Value>,
    upper: Option<&Value>,
    step: i64,
    len: i64,
) -> Vec<usize> {
    use crate::eval::names::{clamp_slice_index, clamp_slice_index_neg};
    let resolve = |v: Option<&Value>, default: i64| -> i64 {
        match v {
            Some(Value::Int(i)) => *i,
            Some(Value::Bool(b)) => i64::from(*b),
            _ => default,
        }
    };
    let mut out = Vec::new();
    if step > 0 {
        let mut i = clamp_slice_index(resolve(lower, 0), len);
        let end = clamp_slice_index(resolve(upper, len), len);
        while i < end {
            if let Ok(u) = usize::try_from(i) {
                out.push(u);
            }
            i += step;
        }
    } else {
        let mut i = clamp_slice_index_neg(resolve(lower, len - 1), len);
        let end = clamp_slice_index_neg(resolve(upper, -(len + 1)), len);
        while i > end {
            if let Ok(u) = usize::try_from(i) {
                out.push(u);
            }
            i += step;
        }
    }
    out.sort_unstable_by(|a, b| b.cmp(a));
    out.dedup();
    out
}
