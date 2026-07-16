// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use rustpython_parser::ast::{self, Expr};

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{eval_expr, place},
    state::{InterpreterState, estimate_value_size},
    tools::Tools,
    value::{Value, shared_list},
};

/// Built-in function names that user code is not allowed to shadow. Shadowing
/// these would erase the interpreter's runtime dispatch table entry for the
/// name and break any subsequent calls that relied on it.
const PROTECTED_BUILTINS: &[&str] = &[
    "print",
    "len",
    "range",
    "str",
    "int",
    "float",
    "bool",
    "type",
    "isinstance",
    "issubclass",
    "super",
    "hasattr",
    "callable",
    "abs",
    "round",
    "min",
    "max",
    "sum",
    "all",
    "any",
    "sorted",
    "enumerate",
    "zip",
    "reversed",
    "chr",
    "ord",
    "list",
    "tuple",
    "dict",
    "set",
    "filter",
    "map",
    "object",
];

/// Evaluate an assignment statement (a = b, a, b = c, d, etc.).
pub async fn eval_assign(
    state: &mut InterpreterState,
    node: &ast::StmtAssign,
    tools: &Tools,
) -> EvalResult {
    let value = eval_expr(state, &node.value, tools).await?;

    if node.targets.len() == 1 {
        // Single target assignment
        assign_target(state, &node.targets[0], value, tools).await?;
    } else {
        // Multiple targets (a = b = val) — assign same value to all targets
        for target in &node.targets {
            assign_target(state, target, value.clone(), tools).await?;
        }
    }

    Ok(Value::None)
}

/// Evaluate an augmented assignment statement (a += b, etc.).
///
/// Python evaluates the target's subscripts once, reads the current value,
/// evaluates the RHS, combines, then writes back to the same slot. The place is
/// resolved first (mirroring that evaluation order), the leaf is read by cloning
/// only the leaf value (not the whole root), and the result is written back
/// in place with an O(1) memory-delta update.
pub async fn eval_aug_assign(
    state: &mut InterpreterState,
    node: &ast::StmtAugAssign,
    tools: &Tools,
) -> EvalResult {
    // Augmented assignment to a class attribute via the class name
    // (`C.count += 1`): the attribute lives in the class registry, which the
    // place machinery (which navigates Values) cannot reach. Read-modify-write
    // it directly.
    if let Expr::Attribute(attr_node) = node.target.as_ref() {
        // Augmented assignment to a class attribute reached through a *computed*
        // class expression (`type(self)._count += 1`) as well as a bare class
        // name (`C.count += 1`): the attribute lives in the class registry, which
        // the place machinery (which navigates Values) cannot reach. A bare
        // `Name` resolving to a class is checked without re-evaluating; any other
        // base expression is evaluated once (CPython evaluates the object once).
        let base_class = match attr_node.value.as_ref() {
            Expr::Name(name_node) => match state.variables.get(name_node.id.as_str()) {
                Some(Value::Class(class_name)) => Some(class_name.clone()),
                _ => None,
            },
            other => match eval_expr(state, other, tools).await? {
                Value::Class(class_name) => Some(class_name),
                _ => None,
            },
        };
        if let Some(class_name) = base_class {
            let attr_name = attr_node.attr.as_str();
            // Read the current value, walking the MRO so an inherited class
            // attribute is visible.
            let current = state.classes.get(&class_name).and_then(|c| {
                c.class_attrs.get(attr_name).cloned().or_else(|| {
                    c.mro.iter().find_map(|anc| {
                        state.classes.get(anc).and_then(|a| a.class_attrs.get(attr_name).cloned())
                    })
                })
            });
            if let Some(current) = current {
                let rhs = eval_expr(state, &node.value, tools).await?;
                let new_value =
                    crate::eval::op::aug_binop(state, node.op, &current, &rhs, tools).await?;
                if let Some(class) = state.classes.get_mut(&class_name) {
                    class.class_attrs.insert(attr_name.to_string(), new_value);
                }
                return Ok(Value::None);
            }
        }
        // Augmented assignment to a function attribute (`counter.count += 1`):
        // the attribute lives in `state.function_attrs`, which the place
        // machinery cannot reach. Read-modify-write it directly.
        if let Expr::Name(name_node) = attr_node.value.as_ref() {
            if let Some(Value::Function(func_def)) = state.variables.get(name_node.id.as_str()) {
                let key = func_def.body_cache_key().to_string();
                let attr_name = attr_node.attr.as_str();
                if let Some(current) =
                    state.function_attrs.get(&key).and_then(|m| m.get(attr_name)).cloned()
                {
                    let rhs = eval_expr(state, &node.value, tools).await?;
                    let new_value =
                        crate::eval::op::aug_binop(state, node.op, &current, &rhs, tools).await?;
                    state
                        .function_attrs
                        .entry(key)
                        .or_default()
                        .insert(attr_name.to_string(), new_value);
                    return Ok(Value::None);
                }
            }
        }
    }

    // Track E: defaultdict pre-touch. If the target is `d[k]` where
    // `d` is a DefaultDict and `k` is missing, synthesise the entry
    // via the factory before the place machinery navigates.
    pretouch_defaultdict(state, &node.target, tools).await?;

    let place = place::eval_place(state, &node.target, tools).await?.ok_or_else(|| {
        EvalError::from(InterpreterError::Runtime(
            "unsupported augmented assignment target (see CONFORMANCE.md#unsupported-language-features)".into(),
        ))
    })?;

    // Slice aug-assign (`a[1:3] += rhs`): CPython evaluates as
    // `a[1:3] = a[1:3] + rhs` — get a fresh slice list, combine, set slice.
    if let Some((place::PlaceStep::Slice(spec), prefix)) = place.steps.split_last() {
        if !prefix.iter().all(|s| !matches!(s, place::PlaceStep::Slice(_))) {
            return Err(InterpreterError::Runtime(
                "augmented assignment to a nested slice target is not supported (see CONFORMANCE.md#unsupported-language-features)".into(),
            )
            .into());
        }
        let current = {
            let root = state
                .variables
                .get_mut(&place.root)
                .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&place.root)))?;
            place::with_navigate_mut(root, prefix, |parent| place::get_slice(parent, spec))??
        };
        let rhs = eval_expr(state, &node.value, tools).await?;
        let new_value = crate::eval::op::aug_binop(state, node.op, &current, &rhs, tools).await?;
        let delta = {
            let root = state
                .variables
                .get_mut(&place.root)
                .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&place.root)))?;
            place::with_navigate_mut(root, prefix, |parent| {
                place::set_slice(parent, spec, new_value)
            })??
        };
        place::apply_mem_delta(state, delta)?;
        return Ok(Value::None);
    }

    if !place.is_navigable() {
        return Err(InterpreterError::Runtime(
            "augmented assignment to a slice target is not supported (see CONFORMANCE.md#unsupported-language-features)".into(),
        )
        .into());
    }

    // Read the current leaf value, then drop the borrow so the RHS can run.
    let current = {
        let root = state
            .variables
            .get_mut(&place.root)
            .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&place.root)))?;
        place::with_navigate_mut(root, &place.steps, |slot| slot.clone())?
    };

    let rhs = eval_expr(state, &node.value, tools).await?;
    let new_value = crate::eval::op::aug_binop(state, node.op, &current, &rhs, tools).await?;

    // A bare-name aug-assign (`i += 1`) writes directly into the slot below,
    // bypassing `set_variable`'s cell write-through — so mirror it here for any
    // capture cell the frame owns, letting a late-binding closure over `i` (the
    // `while i < n: fns.append(lambda: i); i += 1` pattern) see the update.
    let cell_writethrough = place.steps.is_empty().then(|| {
        state
            .frame_cell_owners
            .last()
            .and_then(|owners| owners.get(&place.root).copied())
            .map(|cell_id| (cell_id, new_value.clone()))
    });

    let delta = {
        let root = state
            .variables
            .get_mut(&place.root)
            .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&place.root)))?;
        place::with_navigate_mut(root, &place.steps, |slot| {
            let delta =
                place::size_delta(estimate_value_size(slot), estimate_value_size(&new_value));
            *slot = new_value;
            delta
        })?
    };
    place::apply_mem_delta(state, delta)?;
    if let Some(Some((cell_id, value))) = cell_writethrough {
        state.nonlocal_cells.entry(cell_id).or_default().insert(place.root.clone(), value);
    }

    Ok(Value::None)
}

/// Track E: synthesize a defaultdict entry before the place
/// machinery navigates to it. Called from eval_aug_assign and from
/// the method-call path (`d[k].append(x)`). The traversal walks the
/// expr chain bottom-up — a chain like `outer[a][b][c]` first
/// pre-touches `outer[a]`, then `outer[a][b]`, etc., so nested
/// defaultdicts compose. Caps recursion depth at 16 to avoid
/// pathological input.
pub(crate) async fn pretouch_defaultdict(
    state: &mut InterpreterState,
    target: &Expr,
    tools: &Tools,
) -> Result<(), EvalError> {
    pretouch_defaultdict_inner(state, target, tools, 0).await
}

fn pretouch_defaultdict_inner<'a>(
    state: &'a mut InterpreterState,
    target: &'a Expr,
    tools: &'a Tools,
    depth: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EvalError>> + Send + 'a>> {
    Box::pin(async move {
        if depth > 16 {
            return Ok(());
        }
        let Expr::Subscript(sub) = target else { return Ok(()) };
        // Pre-touch inner subscripts first so the base container exists — this is
        // what makes NESTED defaultdicts (`d[a][b] += 1`) work: `d[a]` is
        // autovivified before we touch `[b]` on it.
        pretouch_defaultdict_inner(state, sub.value.as_ref(), tools, depth + 1).await?;
        // A slice subscript (`x[a:b]`) never names a defaultdict key — skip it
        // (and it would make `value_to_key` fail on the unhashable slice).
        if matches!(sub.slice.as_ref(), Expr::Slice(_)) {
            return Ok(());
        }
        // Resolve the base (`sub.value`) to a place — a bare name resolves to an
        // empty-step place, a subscript to a navigable one — so a defaultdict
        // reached through any chain (not just a bare `d`) is pre-touched.
        let Some(place) = place::eval_place(state, sub.value.as_ref(), tools).await? else {
            return Ok(());
        };
        // A base reached through a slice (`b[::-1][k]`) is not a navigable slot,
        // so it cannot be a defaultdict — skip rather than navigating (which
        // would raise the intermediate-slice error).
        if !place.is_navigable() {
            return Ok(());
        }
        let key = crate::eval::literals::value_to_key(&eval_expr(state, &sub.slice, tools).await?)?;
        // Read the base defaultdict's factory iff the key is absent.
        let factory = {
            let root = state
                .variables
                .get_mut(&place.root)
                .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&place.root)))?;
            place::with_navigate_mut(root, &place.steps, |slot| match slot {
                Value::DefaultDict(data) if !data.items.contains_key(&key) => {
                    Some(data.factory.clone())
                }
                _ => None,
            })?
        };
        let Some(factory) = factory else { return Ok(()) };
        let synth = crate::eval::names::invoke_factory_pub(state, &factory, tools).await?;
        let mut synth = Some(synth);
        let root = state
            .variables
            .get_mut(&place.root)
            .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&place.root)))?;
        place::with_navigate_mut(root, &place.steps, |slot| {
            if let (Value::DefaultDict(data), Some(s)) = (slot, synth.take()) {
                data.items.insert(key.clone(), s);
            }
        })?;
        Ok(())
    })
}

/// Evaluate an annotated assignment statement (x: int = 5).
pub async fn eval_ann_assign(
    state: &mut InterpreterState,
    node: &ast::StmtAnnAssign,
    tools: &Tools,
) -> EvalResult {
    // Only process if there's a value to assign
    if let Some(ref value_expr) = node.value {
        let value = eval_expr(state, value_expr, tools).await?;
        assign_target(state, &node.target, value, tools).await?;
    }
    // Ignore the annotation itself
    Ok(Value::None)
}

/// Unpack an iterable `value` into the `elts` slice, assigning each target.
/// Used by `Expr::Tuple` and `Expr::List` LHS patterns in `assign_target`.
async fn assign_unpacking(
    state: &mut InterpreterState,
    elts: &[Expr],
    value: Value,
    tools: &Tools,
) -> Result<(), EvalError> {
    // Use op::iter so generator unpacking (`a, b = gen()`) advances
    // the Lazy cursor and matches CPython's one-shot semantics.
    // Falls back to value_to_iterable's sync paths for builtins by
    // way of dispatch_iter inside op::iter.
    let items =
        if matches!(value, Value::Lazy { .. } | Value::Generator { .. } | Value::Instance(_)) {
            crate::eval::op::iter(state, &value, tools).await?
        } else {
            value_to_iterable(&value)?
        };

    // PEP 3132 starred unpacking: `a, *b, c = ...` — exactly one `*`
    // target consumes the middle slice. CPython's invariant: there
    // must be at least one item per non-star target; the star
    // collects the remainder as a list (possibly empty).
    let star_index = elts.iter().position(|e| matches!(e, Expr::Starred(_)));
    if let Some(star_idx) = star_index {
        // Reject `a, *b, *c = ...` — two stars in one target list.
        if elts.iter().skip(star_idx + 1).any(|e| matches!(e, Expr::Starred(_))) {
            return Err(InterpreterError::Runtime(
                "multiple starred expressions in assignment".into(),
            )
            .into());
        }
        let non_star = elts.len() - 1;
        if items.len() < non_star {
            return Err(InterpreterError::ValueError(format!(
                "not enough values to unpack (expected at least {non_star}, got {})",
                items.len()
            ))
            .into());
        }
        let tail_count = elts.len() - star_idx - 1;
        let head = &items[..star_idx];
        let star_slice = &items[star_idx..items.len() - tail_count];
        let tail = &items[items.len() - tail_count..];
        for (elem, val) in elts[..star_idx].iter().zip(head.iter()) {
            assign_target(state, elem, val.clone(), tools).await?;
        }
        // `Expr::Starred` wraps the actual target — unwrap before
        // assigning so the target gets a list, not a Starred copy.
        let Expr::Starred(starred) = &elts[star_idx] else {
            unreachable!("star_index was set by matches! on Expr::Starred above");
        };
        assign_target(state, &starred.value, Value::List(shared_list(star_slice.to_vec())), tools)
            .await?;
        for (elem, val) in elts[star_idx + 1..].iter().zip(tail.iter()) {
            assign_target(state, elem, val.clone(), tools).await?;
        }
        return Ok(());
    }

    // CPython distinguishes the two directions: fewer supplied values than
    // targets is "not enough values"; more is "too many values" (which omits
    // the `got` count). Both are ValueError, not RuntimeError.
    if items.len() < elts.len() {
        return Err(InterpreterError::ValueError(format!(
            "not enough values to unpack (expected {}, got {})",
            elts.len(),
            items.len()
        ))
        .into());
    }
    if items.len() > elts.len() {
        return Err(InterpreterError::ValueError(format!(
            "too many values to unpack (expected {})",
            elts.len()
        ))
        .into());
    }
    for (elem, val) in elts.iter().zip(items) {
        assign_target(state, elem, val, tools).await?;
    }
    Ok(())
}

/// Assign a value to a target expression.
///
/// Uses `Box::pin` at recursive call sites to handle tuple/list unpacking.
pub fn assign_target<'a>(
    state: &'a mut InterpreterState,
    target: &'a Expr,
    value: Value,
    tools: &'a Tools,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EvalError>> + Send + 'a>> {
    Box::pin(async move {
        match target {
            Expr::Name(name_node) => {
                let name = name_node.id.as_str();
                // Check if it's a dangerous name
                crate::security::validator::validate_name(
                    crate::security::validator::NameContext::Assignment,
                    name,
                )?;
                // Check if it's a protected tool name
                if tools.contains_key(name) {
                    return Err(InterpreterError::Security(format!(
                        "cannot assign to name '{name}': doing this would erase the existing tool"
                    ))
                    .into());
                }
                if PROTECTED_BUILTINS.contains(&name) {
                    return Err(InterpreterError::Security(format!(
                        "cannot assign to name '{name}': doing this would erase the existing tool"
                    ))
                    .into());
                }
                state.set_variable(name, value).map_err(EvalError::Interpreter)?;
                Ok(())
            }
            Expr::Tuple(tuple_node) => {
                assign_unpacking(state, &tuple_node.elts, value, tools).await
            }
            Expr::List(list_node) => assign_unpacking(state, &list_node.elts, value, tools).await,
            // Attribute assignment goes through @property setter
            // (data descriptor wins), then `__setattr__` (user-defined
            // intercept), then the place machinery (default field
            // write). Property + __setattr__ are async-only paths.
            Expr::Attribute(attr_node) => {
                // Temporary receiver (`getcontext().prec = n`): mutate shared
                // instance fields even when the base is not a bare name, then
                // sync decimal.Context.prec into interpreter state.
                if !matches!(attr_node.value.as_ref(), Expr::Name(_)) {
                    let obj = crate::eval::eval_expr(state, &attr_node.value, tools).await?;
                    if let Value::Instance(inst) = &obj {
                        let attr_name = attr_node.attr.as_str();
                        if state.classes.get(&inst.class_name).is_some_and(|c| c.frozen) {
                            return Err(EvalError::Exception(crate::value::ExceptionValue::new(
                                "FrozenInstanceError",
                                format!("cannot assign to field '{attr_name}'"),
                            )));
                        }
                        inst.fields.lock().insert(attr_name.to_string(), value.clone());
                        if attr_name == "prec"
                            && inst.class_name == crate::eval::modules::decimal::CONTEXT_CLASS
                        {
                            if let Value::Int(n) = &value {
                                if *n >= 1 {
                                    state.decimal_prec = *n;
                                }
                            }
                        }
                        return Ok(());
                    }
                    return Err(InterpreterError::Runtime(
                        "cannot assign to this expression".into(),
                    )
                    .into());
                }
                if let Expr::Name(name_node) = attr_node.value.as_ref() {
                    let obj_name = name_node.id.as_str().to_string();
                    if let Some(obj) = state.variables.get(&obj_name).cloned() {
                        if let Value::Instance(inst) = &obj {
                            let class_name = inst.class_name.clone();
                            let attr_name = attr_node.attr.as_str().to_string();
                            // `@dataclass(frozen=True)` — reject field writes.
                            if state.classes.get(&class_name).is_some_and(|c| c.frozen) {
                                return Err(EvalError::Exception(
                                    crate::value::ExceptionValue::new(
                                        "FrozenInstanceError",
                                        format!("cannot assign to field '{attr_name}'"),
                                    ),
                                ));
                            }
                            // `__slots__` / `@dataclass(slots=True)` allowlist.
                            if let Some(class) = state.classes.get(&class_name) {
                                if class.slots {
                                    let allowed = class.slot_names.iter().any(|n| n == &attr_name)
                                        || class.dataclass_fields.as_ref().is_some_and(|fs| {
                                            fs.iter().any(|f| f.name == attr_name)
                                        });
                                    if !allowed {
                                        return Err(InterpreterError::AttributeError(format!(
                                            "'{class_name}' object has no attribute '{attr_name}'"
                                        ))
                                        .into());
                                    }
                                }
                            }
                            // User data descriptor `__set__` on class attrs.
                            if let Some(desc) = crate::eval::classes::lookup_class_attr_instance(
                                state,
                                &class_name,
                                &attr_name,
                            ) {
                                if let Some((_, set_method)) =
                                    crate::eval::classes::lookup_method_in_mro(
                                        state,
                                        &desc.class_name,
                                        "__set__",
                                    )
                                {
                                    let call = crate::eval::functions::CallArgs {
                                        positional: &[obj.clone(), value.clone()],
                                        keyword: &indexmap::IndexMap::new(),
                                    };
                                    let _ = crate::eval::classes::call_method(
                                        state,
                                        &set_method,
                                        Value::Instance(desc),
                                        call,
                                        tools,
                                    )
                                    .await?;
                                    return Ok(());
                                }
                            }
                            if let Some(prop) = crate::eval::classes::lookup_property(
                                state,
                                &class_name,
                                &attr_name,
                            ) {
                                if let Some(setter) = prop.setter {
                                    let configured = crate::eval::classes::invoke_property_setter(
                                        state, &setter, obj, value, tools,
                                    )
                                    .await?;
                                    state
                                        .set_variable(&obj_name, configured)
                                        .map_err(EvalError::Interpreter)?;
                                    return Ok(());
                                }
                                if prop.setter.is_none() {
                                    return Err(InterpreterError::AttributeError(format!(
                                        "property '{attr_name}' of '{class_name}' object has no setter"
                                    ))
                                    .into());
                                }
                            }
                            if let Some((_, method)) = crate::eval::classes::lookup_method_in_mro(
                                state,
                                &class_name,
                                "__setattr__",
                            ) {
                                let name_arg = Value::String(attr_name.into());
                                let call = crate::eval::functions::CallArgs {
                                    positional: &[name_arg, value.clone()],
                                    keyword: &indexmap::IndexMap::new(),
                                };
                                let (_returned, updated_self) = crate::eval::classes::call_method(
                                    state, &method, obj, call, tools,
                                )
                                .await?;
                                state
                                    .set_variable(&obj_name, updated_self)
                                    .map_err(EvalError::Interpreter)?;
                                return Ok(());
                            }
                        }
                        // Assigning to a class attribute via the class name
                        // (`C.attr = v`) mutates the class registry, not a
                        // Value field — the place machinery can't reach it.
                        if let Value::Class(class_name) = &obj {
                            let attr_name = attr_node.attr.as_str().to_string();
                            if let Some(class) = state.classes.get_mut(class_name) {
                                class.class_attrs.insert(attr_name, value.clone());
                                return Ok(());
                            }
                        }
                        // Function objects carry an arbitrary attribute
                        // namespace (`func.attr = value`) — used by decorator
                        // patterns that stash call counters or registries on the
                        // wrapper. Keyed by `body_key` so all clones of the def
                        // (`g = f; g.x = 1`) share one namespace, as CPython's
                        // per-function `__dict__` does. Reject the dunder names
                        // that report from the FunctionDef itself.
                        if let Value::Function(func_def) = &obj {
                            let attr_name = attr_node.attr.as_str();
                            let key = func_def.body_cache_key().to_string();
                            state
                                .function_attrs
                                .entry(key)
                                .or_default()
                                .insert(attr_name.to_string(), value.clone());
                            return Ok(());
                        }
                        // Exception instances carry an arbitrary attribute
                        // namespace (`exc.custom = ...`), stored in the boxed
                        // `ExceptionValue::fields` — the read side already
                        // resolves user fields. Mutate the stored value in place
                        // (exceptions are value-semantic, not `Arc`-shared).
                        if matches!(&obj, Value::Exception(_)) {
                            let attr_name = attr_node.attr.as_str();
                            crate::security::validator::validate_attribute(attr_name)?;
                            if let Some(Value::Exception(exc)) = state.variables.get_mut(&obj_name)
                            {
                                exc.fields.insert(attr_name.to_string(), value.clone());
                                return Ok(());
                            }
                        }
                    }
                }
                // Fall through to the place machinery.
                let target_place =
                    place::eval_place(state, target, tools).await?.ok_or_else(|| {
                        EvalError::from(InterpreterError::Runtime(
                            "cannot assign to this expression".into(),
                        ))
                    })?;
                let Some((last, prefix)) = target_place.steps.split_last() else {
                    return Err(InterpreterError::Runtime(
                        "assignment target resolved to a bare name unexpectedly".into(),
                    )
                    .into());
                };
                let delta = {
                    let root = state.variables.get_mut(&target_place.root).ok_or_else(|| {
                        EvalError::from(InterpreterError::name_not_defined(&target_place.root))
                    })?;
                    place::with_navigate_mut(root, prefix, |container| {
                        place::assign_terminal(container, last, value.clone())
                    })??
                };
                place::apply_mem_delta(state, delta)?;
                // decimal.Context.prec writeback into interpreter state.
                if let place::PlaceStep::Attr(attr) = last {
                    if prefix.is_empty() && attr == "prec" {
                        let class_name = state.variables.get(&target_place.root).and_then(|v| {
                            if let Value::Instance(inst) = v {
                                Some(inst.class_name.clone())
                            } else {
                                None
                            }
                        });
                        if class_name.as_deref()
                            == Some(crate::eval::modules::decimal::CONTEXT_CLASS)
                        {
                            if let Value::Int(n) = &value {
                                if *n >= 1 {
                                    state.decimal_prec = *n;
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
            // Subscript / slice assignment goes through the place
            // system for builtin containers (resolves the full target
            // path `d["a"]["x"]`, `lst[1:]` and mutates the slot in
            // place) or through `op::setitem` for user-class instances
            // (calls `__setitem__` and writes the post-call self back).
            Expr::Subscript(sub) => {
                let Some(target_place) = place::eval_place(state, target, tools).await? else {
                    // Non-place receiver (`f()[x] = v`, `(1, 2)[0] = 5`):
                    // the target isn't rooted at a name, so there's
                    // nothing to write back. User instances dispatch
                    // `__setitem__`; builtin containers mutate through
                    // their shared handle (or raise TypeError when
                    // immutable) so a literal tuple/str assignment fails
                    // exactly as CPython does rather than emitting a
                    // misleading "cannot assign" RuntimeError.
                    let mut receiver = crate::eval::eval_expr(state, &sub.value, tools).await?;
                    if matches!(receiver, Value::Instance(_)) {
                        let key = crate::eval::eval_expr(state, &sub.slice, tools).await?;
                        if crate::eval::op::setitem(state, &receiver, &key, value, tools)
                            .await?
                            .is_none()
                        {
                            return Err(InterpreterError::TypeError(format!(
                                "'{}' object does not support item assignment",
                                receiver.type_name()
                            ))
                            .into());
                        }
                        return Ok(());
                    }
                    return place::assign_computed_subscript(
                        state,
                        &mut receiver,
                        &sub.slice,
                        value,
                        tools,
                    )
                    .await;
                };
                // A bare `Name` is handled by the arm above, so a subscript /
                // attribute target always carries at least one step.
                let Some((last, prefix)) = target_place.steps.split_last() else {
                    return Err(InterpreterError::Runtime(
                        "assignment target resolved to a bare name unexpectedly".into(),
                    )
                    .into());
                };

                // Fast path for the common builtin case: inspect the terminal
                // receiver in place instead of cloning the whole receiver just
                // to check whether user `__setitem__` applies. The old path made
                // `d[i] = v` O(n²) for growing dicts because each assignment
                // cloned the entire dict before routing back to `assign_terminal`.
                if let place::PlaceStep::Index(key) = last {
                    // A dict with an instance key needs async `__hash__`/`__eq__`
                    // insertion (the sync `value_to_key` path reports the
                    // instance as unhashable). Snapshot the target dict, insert,
                    // and write back to the same shared handle.
                    if matches!(key, Value::Instance(_)) {
                        let dict_handle = {
                            let root =
                                state.variables.get_mut(&target_place.root).ok_or_else(|| {
                                    EvalError::from(InterpreterError::name_not_defined(
                                        &target_place.root,
                                    ))
                                })?;
                            place::with_navigate_mut(root, prefix, |container| match container {
                                Value::Dict(map) => Some(map.clone()),
                                _ => None,
                            })?
                        };
                        if let Some(map) = dict_handle {
                            let mut snapshot = map.lock().clone();
                            crate::eval::op::dict_insert_instance_key_pub(
                                state,
                                &mut snapshot,
                                key,
                                value,
                                tools,
                            )
                            .await?;
                            map.lock().set_map(snapshot);
                            return Ok(());
                        }
                    }
                    let receiver = {
                        let root =
                            state.variables.get_mut(&target_place.root).ok_or_else(|| {
                                EvalError::from(InterpreterError::name_not_defined(
                                    &target_place.root,
                                ))
                            })?;
                        place::with_navigate_mut(root, prefix, |container| {
                            if matches!(container, Value::Instance(_)) {
                                Some(container.clone())
                            } else {
                                None
                            }
                        })?
                    };
                    if let Some(receiver) = receiver {
                        if let Some(updated_self) =
                            crate::eval::op::setitem(state, &receiver, key, value.clone(), tools)
                                .await?
                        {
                            let delta = {
                                let root = state.variables.get_mut(&target_place.root).ok_or_else(
                                    || {
                                        EvalError::from(InterpreterError::name_not_defined(
                                            &target_place.root,
                                        ))
                                    },
                                )?;
                                place::with_navigate_mut(root, prefix, |container| {
                                    let delta = place::size_delta(
                                        estimate_value_size(container),
                                        estimate_value_size(&updated_self),
                                    );
                                    *container = updated_self;
                                    delta
                                })?
                            };
                            place::apply_mem_delta(state, delta)?;
                            return Ok(());
                        }
                    }
                }

                // A user-class instance takes `obj[i:j] = v` via
                // `__setitem__(slice(i, j), v)`, not the builtin slice assign.
                if let place::PlaceStep::Slice(spec) = last {
                    let receiver = {
                        let root =
                            state.variables.get_mut(&target_place.root).ok_or_else(|| {
                                EvalError::from(InterpreterError::name_not_defined(
                                    &target_place.root,
                                ))
                            })?;
                        place::with_navigate_mut(root, prefix, |container| {
                            matches!(container, Value::Instance(_)).then(|| container.clone())
                        })?
                    };
                    if let Some(receiver) = receiver {
                        let slice_val = Value::Slice(Box::new(crate::value::SliceValue {
                            start: spec.lower.clone().unwrap_or(Value::None),
                            stop: spec.upper.clone().unwrap_or(Value::None),
                            step: spec.step.clone().unwrap_or(Value::None),
                        }));
                        if let Some(updated_self) = crate::eval::op::setitem(
                            state,
                            &receiver,
                            &slice_val,
                            value.clone(),
                            tools,
                        )
                        .await?
                        {
                            let delta = {
                                let root = state.variables.get_mut(&target_place.root).ok_or_else(
                                    || {
                                        EvalError::from(InterpreterError::name_not_defined(
                                            &target_place.root,
                                        ))
                                    },
                                )?;
                                place::with_navigate_mut(root, prefix, |container| {
                                    let delta = place::size_delta(
                                        estimate_value_size(container),
                                        estimate_value_size(&updated_self),
                                    );
                                    *container = updated_self;
                                    delta
                                })?
                            };
                            place::apply_mem_delta(state, delta)?;
                            return Ok(());
                        }
                    }
                }

                let delta = {
                    let root = state.variables.get_mut(&target_place.root).ok_or_else(|| {
                        EvalError::from(InterpreterError::name_not_defined(&target_place.root))
                    })?;
                    place::with_navigate_mut(root, prefix, |container| {
                        place::assign_terminal(container, last, value)
                    })??
                };
                place::apply_mem_delta(state, delta)?;
                Ok(())
            }
            _ => Err(InterpreterError::Runtime(format!(
                "unsupported assignment target: {:?}",
                std::mem::discriminant(target)
            ))
            .into()),
        }
    })
}

/// Convert a value to an iterable Vec for unpacking.
fn value_to_iterable(val: &Value) -> Result<Vec<Value>, EvalError> {
    match val {
        // List is shared via Arc<Mutex<Vec>>; clone the inner Vec under
        // the lock to snapshot for unpacking. Tuple/Set wrap plain Vec.
        Value::List(items) => Ok(items.lock().clone()),
        Value::Tuple(items) => Ok(items.clone()),
        // Sets unpack in CPython's hash-table iteration order (`a, b = {..}`),
        // like every other set observation.
        Value::Set(b) => Ok(b.lock().iter_ordered()),
        Value::Frozenset(b) => Ok(b.iter_ordered()),
        Value::String(s) => Ok(s.chars().map(|c| Value::String(c.to_string().into())).collect()),
        Value::Range { start, stop, step } => {
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
        // Everything else iterable — dict views (`a, b = d.items()`), dicts
        // (yielding keys), deque, bytes/bytearray, etc. — routes through the
        // shared type-layer iterator; only a genuinely non-iterable value
        // raises the unpack-specific TypeError.
        _ => crate::types::dispatch_iter(val).map_err(|_| {
            EvalError::from(InterpreterError::TypeError(format!(
                "cannot unpack non-iterable {} object",
                val.type_name()
            )))
        }),
    }
}
