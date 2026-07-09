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
        // `del inst.attr` — Track B2 supports the @property deleter
        // path. Other attribute deletions are out of scope: instance
        // fields have no del path in our owned model, and __delattr__
        // beyond the property protocol is not modelled.
        Expr::Attribute(attr_node) => {
            let Expr::Name(name_node) = attr_node.value.as_ref() else {
                return Err(InterpreterError::Runtime(
                    "complex delete attribute target not supported (see CONFORMANCE.md#unsupported-language-features)".into(),
                )
                .into());
            };
            let obj_name = name_node.id.as_str().to_string();
            let attr_name = attr_node.attr.as_str().to_string();
            let obj =
                state.variables.get(&obj_name).cloned().ok_or_else(|| {
                    EvalError::from(InterpreterError::name_not_defined(&obj_name))
                })?;
            let Value::Instance(inst) = &obj else {
                return Err(InterpreterError::AttributeError(format!(
                    "'{}' object attribute deletion not supported",
                    obj.type_name()
                ))
                .into());
            };
            let class_name = inst.class_name.clone();
            if let Some(prop) =
                crate::eval::classes::lookup_property(state, &class_name, &attr_name)
            {
                let Some(deleter) = prop.deleter else {
                    return Err(InterpreterError::AttributeError(format!(
                        "property '{attr_name}' of '{class_name}' object has no deleter"
                    ))
                    .into());
                };
                let configured =
                    crate::eval::classes::invoke_property_deleter(state, &deleter, obj, tools)
                        .await?;
                state.set_variable(&obj_name, configured).map_err(EvalError::Interpreter)?;
                return Ok(());
            }
            // User-defined `__delattr__` intercepts every plain field
            // delete. Inside the slot, `super().__delattr__(name)` is
            // the canonical way to actually drop the field — that's
            // dispatched by classes::super_method_call's object-default
            // path.
            if let Some((_, method)) =
                crate::eval::classes::lookup_method_in_mro(state, &class_name, "__delattr__")
            {
                let name_arg = Value::String(attr_name.as_str().into());
                let call = crate::eval::functions::CallArgs {
                    positional: std::slice::from_ref(&name_arg),
                    keyword: &indexmap::IndexMap::new(),
                };
                let (_returned, updated_self) =
                    crate::eval::classes::call_method(state, &method, obj, call, tools).await?;
                state.set_variable(&obj_name, updated_self).map_err(EvalError::Interpreter)?;
                return Ok(());
            }
            // No __delattr__ slot: default is to drop the field
            // directly. CPython's `object.__delattr__` raises
            // AttributeError when the field is missing.
            if inst.fields.lock().contains_key(&attr_name) {
                let new_inst = inst.clone();
                new_inst.fields.lock().remove(&attr_name);
                state
                    .set_variable(&obj_name, Value::Instance(new_inst))
                    .map_err(EvalError::Interpreter)?;
                return Ok(());
            }
            Err(InterpreterError::AttributeError(format!(
                "'{class_name}' object has no attribute '{attr_name}'"
            ))
            .into())
        }
        Expr::Subscript(sub_node) => {
            // Slice deletion is its own path — keeps the Name-only fast
            // path for `del lst[1:3]` and similar list-slice idioms.
            if let Expr::Slice(slice_node) = sub_node.slice.as_ref() {
                if let Expr::Name(name_node) = sub_node.value.as_ref() {
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
            let s = clamp(&start, 0)?;
            let e = clamp(&stop, items_len)?;

            let result = if s < e && s < items_len {
                let end = e.min(items_len);
                let drained_size: usize =
                    guard[s..end].iter().map(crate::state::estimate_value_size).sum();
                guard.drain(s..end);
                Ok(drained_size)
            } else {
                Ok(0)
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
