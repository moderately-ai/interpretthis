// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use indexmap::IndexMap;
use rustpython_parser::ast::{self, Expr};

use super::{
    builtins::{is_exception_type_name, try_builtin},
    dispatch::{call_lambda, call_user_function, call_value_as_function},
    helpers::{SortRequest, dsu_sort, list_sort_type_error},
    method_dispatch::{CallArgs, dispatch_method, resolve_method_args, resolve_method_kwargs},
};
use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{eval_expr, place},
    state::{InterpreterState, estimate_value_size},
    tools::Tools,
    value::{ExceptionValue, Value, ValueKey},
};

/// Evaluate a function call expression.
pub async fn eval_call(
    state: &mut InterpreterState,
    node: &ast::ExprCall,
    tools: &Tools,
) -> EvalResult {
    // Resolve the function name for dispatch
    let (func_name, is_method_call, method_obj_expr) = resolve_func_info(&node.func);

    // Evaluate positional arguments
    let mut args = Vec::new();
    for arg_expr in &node.args {
        if let Expr::Starred(starred) = arg_expr {
            // *args unpacking
            let val = eval_expr(state, &starred.value, tools).await?;
            let items = crate::eval::op::iter(state, &val, tools).await?;
            args.extend(items);
        } else {
            args.push(eval_expr(state, arg_expr, tools).await?);
        }
    }

    // Evaluate keyword arguments
    let mut kwargs: IndexMap<String, Value> = IndexMap::new();
    for kw in &node.keywords {
        if let Some(ref arg_name) = kw.arg {
            let val = eval_expr(state, &kw.value, tools).await?;
            kwargs.insert(arg_name.as_str().to_string(), val);
        } else {
            // **kwargs unpacking
            let val = eval_expr(state, &kw.value, tools).await?;
            if let Value::Dict(map) = val {
                let snapshot = map.lock().clone();
                for (k, v) in snapshot {
                    // A non-string key raises `TypeError: keywords must be
                    // strings` — it was previously skipped silently, so
                    // `f(**{1: 2})` quietly passed no arguments.
                    let ValueKey::String(key_str) = k else {
                        return Err(
                            InterpreterError::TypeError("keywords must be strings".into()).into()
                        );
                    };
                    kwargs.insert(key_str.into(), v);
                }
            } else {
                return Err(InterpreterError::TypeError(
                    "** operator requires a dictionary".into(),
                )
                .into());
            }
        }
    }

    // Method call dispatch (obj.method())
    if is_method_call {
        if let Some(obj_expr) = method_obj_expr {
            let method_name = func_name.as_deref().unwrap_or("");
            let resolved_args = resolve_method_args(&args).await?;
            let kwargs = resolve_method_kwargs(&kwargs).await?;

            // `str.format` / `str.format_map` are printf-equivalent string
            // building, not a security risk. They are special-cased here because
            // format consumes free-form field kwargs (not a fixed param list),
            // and format_map takes a mapping argument. They never mutate the
            // receiver, so it is evaluated by value.
            if matches!(method_name, "format" | "format_map") {
                let obj = eval_expr(state, obj_expr, tools).await?;
                let Value::String(template) = obj else {
                    return Err(InterpreterError::AttributeError(format!(
                        "'{}' object has no attribute '{method_name}'",
                        obj.type_name()
                    ))
                    .into());
                };
                if method_name == "format" {
                    return crate::eval::strings::str_format(
                        state,
                        &template,
                        &resolved_args,
                        &kwargs,
                        tools,
                    )
                    .await;
                }
                // format_map: take the single mapping argument as the keywords.
                let mapping = resolved_args.first().and_then(Value::as_dict).ok_or_else(|| {
                    EvalError::from(InterpreterError::TypeError(
                        "format_map() requires a mapping argument".into(),
                    ))
                })?;
                let kw: IndexMap<String, Value> = mapping
                    .lock()
                    .iter()
                    .filter_map(|(k, v)| match k {
                        ValueKey::String(s) => Some((s.as_str().to_string(), v.clone())),
                        _ => None,
                    })
                    .collect();
                return crate::eval::strings::str_format(state, &template, &[], &kw, tools).await;
            }

            // list.count / index / remove need async `__eq__` when elements
            // or the needle are user-class instances. The sync method table
            // only has structural `values_equal`.
            if matches!(method_name, "count" | "index" | "remove") {
                if let Some(result) =
                    list_eq_method(state, obj_expr, method_name, &resolved_args, &kwargs, tools)
                        .await?
                {
                    return Ok(result);
                }
            }

            // `list.sort()` is special-cased: CPython 3.12 makes both
            // `key=` and `reverse=` keyword-only, and key= needs async
            // call_value_as_function. Shares `dsu_sort` with `sorted`.
            if method_name == "sort" {
                if !resolved_args.is_empty() {
                    return Err(InterpreterError::TypeError(
                        "sort() takes no positional arguments".into(),
                    )
                    .into());
                }
                let key_fn = kwargs.get("key").cloned();
                let reverse = kwargs.get("reverse").is_some_and(Value::is_truthy);

                // Two paths converge on `items: Vec<Value>`:
                //   * Place receiver (`xs.sort()` where xs is a variable / index path): mem::take
                //     from the navigated slot so dsu_sort can hold &mut state across its await
                //     chain. The sorted Vec is written back via a second navigate after the await —
                //     CPython mutates the list in place, so downstream code observing xs sees the
                //     order.
                //   * Temporary receiver (`[1,2].sort()`, `f().sort()`): destructure the owned
                //     Value. No write-back path; matches CPython where the temp is unobservable.
                let raw_place = place::eval_place(state, obj_expr, tools).await?;
                let usable_place =
                    raw_place.filter(|p| p.is_navigable() && state.variables.contains_key(&p.root));

                let items: Vec<Value> = if let Some(place) = &usable_place {
                    let root = state.variables.get_mut(&place.root).ok_or_else(|| {
                        EvalError::from(InterpreterError::name_not_defined(&place.root))
                    })?;
                    place::with_navigate_mut(root, &place.steps, |target| {
                        let Value::List(items) = target else {
                            return Err(list_sort_type_error(target.type_name()));
                        };
                        // Take the contents out under the lock — the
                        // SharedList stays valid and any aliases see an
                        // empty list while the sort is in flight, then
                        // the sorted contents get written back below.
                        Ok(std::mem::take(&mut *items.lock()))
                    })??
                } else {
                    let obj = eval_expr(state, obj_expr, tools).await?;
                    let Value::List(items) = obj else {
                        return Err(list_sort_type_error(obj.type_name()));
                    };
                    // Temporary receiver — extract the Vec; uniquely
                    // owned avoids a clone, aliased clones the contents.
                    match std::sync::Arc::try_unwrap(items) {
                        Ok(mutex) => mutex.into_inner(),
                        Err(shared) => shared.lock().clone(),
                    }
                };

                let sorted =
                    dsu_sort(state, tools, SortRequest { items, key_fn: key_fn.as_ref(), reverse })
                        .await?;

                if let Some(place) = usable_place {
                    let root = state.variables.get_mut(&place.root).ok_or_else(|| {
                        EvalError::from(InterpreterError::name_not_defined(&place.root))
                    })?;
                    place::with_navigate_mut(root, &place.steps, |target| {
                        if let Value::List(items) = target {
                            *items.lock() = sorted;
                        }
                    })?;
                }
                return Ok(Value::None);
            }

            // Lvalue receiver (`groups[1].append(5)`, `p.method()`): navigate a
            // single `&mut` borrow to the real slot. A built-in container method
            // mutates it in place with an O(1) memory delta; an instance method
            // runs through `call_method` and the mutated `self` is written back.
            // Neither path clones the root.
            // Only an actual variable can be navigated as a place; auto-imported
            // modules (`json`, `re`, `datetime`) are resolved on lookup, not
            // stored, so they fall through to the temporary path below.
            //
            // Track E: pre-touch defaultdict entries on the receiver path so
            // `d[key].append(x)` synthesises the missing entry before navigate.
            crate::eval::statements::pretouch_defaultdict(state, obj_expr, tools).await?;
            if let Some(place) = place::eval_place(state, obj_expr, tools).await? {
                if place.is_navigable() && state.variables.contains_key(&place.root) {
                    // Classify the receiver while holding the borrow, then act
                    // after it is released — an instance method call is async and
                    // needs `&mut state` again.
                    enum Dispatch {
                        Done(Value, isize),
                        Instance(Value),
                        Module(String),
                        /// Module constructor acting as a type
                        /// (`from datetime import datetime` → ModuleFunction).
                        ModuleType {
                            module: String,
                            type_name: String,
                        },
                        Class(String),
                        /// Eager generator buffer (`Value::Lazy`) + protocol method.
                        Generator {
                            receiver: Value,
                            method: String,
                        },
                        /// ExceptionGroup.subgroup / .split, etc.
                        Exception {
                            receiver: Value,
                            method: String,
                        },
                    }
                    // Place navigation only models Instance/Dict/List slots.
                    // `module.member.method(...)` (Attr step on Module) cannot
                    // navigate — fall through to the temp path, which evaluates
                    // the Attribute via module_member (same as eval_attribute).
                    let dispatch: Option<Dispatch> = {
                        let root = state.variables.get_mut(&place.root).ok_or_else(|| {
                            EvalError::from(InterpreterError::name_not_defined(&place.root))
                        })?;
                        match place::with_navigate_mut(root, &place.steps, |target| match target {
                            Value::Instance(_) => {
                                Ok::<Dispatch, EvalError>(Dispatch::Instance(target.clone()))
                            }
                            Value::Module(module) => Ok(Dispatch::Module(module.clone())),
                            Value::ModuleFunction { module, name } => Ok(Dispatch::ModuleType {
                                module: module.clone(),
                                type_name: name.clone(),
                            }),
                            Value::Class(class_name) => Ok(Dispatch::Class(class_name.clone())),
                            Value::Lazy { .. } | Value::Generator { .. }
                                if super::generators::is_generator_method(method_name) =>
                            {
                                Ok(Dispatch::Generator {
                                    receiver: target.clone(),
                                    method: method_name.to_string(),
                                })
                            }
                            Value::Exception(_) => Ok(Dispatch::Exception {
                                receiver: target.clone(),
                                method: method_name.to_string(),
                            }),
                            _ => {
                                let outcome =
                                    dispatch_method(target, method_name, &resolved_args, &kwargs)?;
                                Ok(Dispatch::Done(outcome.value, outcome.mem_delta))
                            }
                        }) {
                            Ok(inner) => Some(inner?),
                            Err(_) => None,
                        }
                    };
                    if let Some(dispatch) = dispatch {
                        match dispatch {
                            Dispatch::Done(value, mem_delta) => {
                                place::apply_mem_delta(state, mem_delta)?;
                                return Ok(value);
                            }
                            Dispatch::Generator { receiver, method } => {
                                return super::generators::dispatch_generator_method(
                                    state,
                                    &receiver,
                                    &method,
                                    &resolved_args,
                                    &kwargs,
                                    tools,
                                )
                                .await;
                            }
                            Dispatch::Exception { receiver, method } => {
                                let Value::Exception(exc) = receiver else {
                                    return Err(InterpreterError::Runtime(
                                        "internal: Exception dispatch without Exception value"
                                            .into(),
                                    )
                                    .into());
                                };
                                return crate::eval::exceptions::call_exception_method(
                                    &method,
                                    &exc,
                                    &resolved_args,
                                );
                            }
                            Dispatch::Module(module) => {
                                return crate::eval::modules::call_function(
                                    state,
                                    &module,
                                    method_name,
                                    &resolved_args,
                                    &kwargs,
                                    tools,
                                )
                                .await;
                            }
                            Dispatch::ModuleType { module, type_name } => {
                                // `datetime.strptime(...)` after
                                // `from datetime import datetime`.
                                let Some(func) = crate::eval::modules::type_classmethod(
                                    &module,
                                    &type_name,
                                    method_name,
                                ) else {
                                    return Err(InterpreterError::AttributeError(format!(
                                        "type object '{type_name}' has no attribute '{method_name}'"
                                    ))
                                    .into());
                                };
                                return crate::eval::modules::call_function(
                                    state,
                                    &module,
                                    func,
                                    &resolved_args,
                                    &kwargs,
                                    tools,
                                )
                                .await;
                            }
                            Dispatch::Class(class_name) => {
                                // Class.method(...) — Track B2:
                                //   * staticmethod: call without receiver
                                //   * classmethod: call with the class as first arg (bound by
                                //     call_method)
                                // Regular instance methods cannot be called
                                // unbound through the class (CPython raises
                                // TypeError "missing 1 required positional
                                // argument: 'self'" when the user forgets).
                                // We surface the same error shape by
                                // falling through to the unbound-call attempt
                                // and letting param binding fail.
                                if let Some(def) = crate::eval::classes::lookup_static_method(
                                    state,
                                    &class_name,
                                    method_name,
                                ) {
                                    return call_user_function(
                                        state,
                                        &def,
                                        &resolved_args,
                                        &kwargs,
                                        tools,
                                    )
                                    .await;
                                }
                                if let Some(def) = crate::eval::classes::lookup_class_method(
                                    state,
                                    &class_name,
                                    method_name,
                                ) {
                                    let call =
                                        CallArgs { positional: &resolved_args, keyword: &kwargs };
                                    let (returned, _self) = crate::eval::classes::call_method(
                                        state,
                                        &def,
                                        Value::Class(class_name.clone()),
                                        call,
                                        tools,
                                    )
                                    .await?;
                                    return Ok(returned);
                                }
                                return Err(InterpreterError::AttributeError(format!(
                                    "type object '{class_name}' has no attribute '{method_name}'"
                                ))
                                .into());
                            }
                            Dispatch::Instance(instance) => {
                                let call =
                                    CallArgs { positional: &resolved_args, keyword: &kwargs };
                                let (returned, configured_self) =
                                    crate::eval::classes::instance_method_call(
                                        state,
                                        instance,
                                        method_name,
                                        call,
                                        tools,
                                    )
                                    .await?;
                                let delta = {
                                    let root =
                                        state.variables.get_mut(&place.root).ok_or_else(|| {
                                            EvalError::from(InterpreterError::name_not_defined(
                                                &place.root,
                                            ))
                                        })?;
                                    place::with_navigate_mut(root, &place.steps, |slot| {
                                        let delta = place::size_delta(
                                            estimate_value_size(slot),
                                            estimate_value_size(&configured_self),
                                        );
                                        *slot = configured_self;
                                        delta
                                    })?
                                };
                                place::apply_mem_delta(state, delta)?;
                                return Ok(returned);
                            }
                        }
                    } // if let Some(dispatch)
                }
            }

            // Non-lvalue receiver (literal, call result, or a slice expression):
            // dispatch against a temporary. Any mutation affects only the
            // discarded value, matching CPython where `[1, 2].append(3)` mutates
            // an object that is immediately thrown away.
            let mut temp = eval_expr(state, obj_expr, tools).await?;
            if matches!(temp, Value::Lazy { .. } | Value::Generator { .. })
                && super::generators::is_generator_method(method_name)
            {
                return super::generators::dispatch_generator_method(
                    state,
                    &temp,
                    method_name,
                    &resolved_args,
                    &kwargs,
                    tools,
                )
                .await;
            }
            if matches!(temp, Value::Instance(_)) {
                let call = CallArgs { positional: &resolved_args, keyword: &kwargs };
                let (returned, _self) = crate::eval::classes::instance_method_call(
                    state,
                    temp,
                    method_name,
                    call,
                    tools,
                )
                .await?;
                return Ok(returned);
            }
            // super().method(...): walk the MRO starting at the slot
            // AFTER defining_class. The receiver passed to the method
            // is the original instance, not the Super proxy — matches
            // CPython's bound-method-with-overridden-MRO behaviour.
            if let Value::Super { defining_class, instance } = &temp {
                let call = CallArgs { positional: &resolved_args, keyword: &kwargs };
                let recv = crate::eval::classes::SuperReceiver {
                    defining_class,
                    instance: (**instance).clone(),
                };
                let (returned, _self) =
                    crate::eval::classes::super_method_call(state, recv, method_name, call, tools)
                        .await?;
                return Ok(returned);
            }
            if let Value::Module(module) = &temp {
                let module_name = module.clone();
                return crate::eval::modules::call_function(
                    state,
                    &module_name,
                    method_name,
                    &resolved_args,
                    &kwargs,
                    tools,
                )
                .await;
            }
            // Module constructor as type: `datetime.datetime.strptime(...)`
            // or `(from datetime import datetime); datetime.strptime(...)`.
            if let Value::ModuleFunction { module, name: type_name } = &temp {
                let Some(func) =
                    crate::eval::modules::type_classmethod(module, type_name, method_name)
                else {
                    return Err(InterpreterError::AttributeError(format!(
                        "type object '{type_name}' has no attribute '{method_name}'"
                    ))
                    .into());
                };
                let module_name = module.clone();
                return crate::eval::modules::call_function(
                    state,
                    &module_name,
                    func,
                    &resolved_args,
                    &kwargs,
                    tools,
                )
                .await;
            }
            // Type-as-receiver classmethod: `dict.fromkeys(iterable,
            // value)`. The receiver is a BuiltinName for the type; we
            // route to the classmethod-aware handler in
            // call_value_as_function.
            if let Value::BuiltinName(type_name) = &temp {
                let unbound = Value::BuiltinTypeMethod {
                    type_name: type_name.clone(),
                    method: method_name.to_string(),
                };
                return call_value_as_function(state, &unbound, &resolved_args, &kwargs, tools)
                    .await;
            }
            if let Value::Exception(exc) = &temp {
                return crate::eval::exceptions::call_exception_method(
                    method_name,
                    exc,
                    &resolved_args,
                );
            }
            return Ok(dispatch_method(&mut temp, method_name, &resolved_args, &kwargs)?.value);
        }
    }

    // Computed callable: the func is an expression (another call, a lambda
    // literal, a subscript, ...), not a bare name or `obj.method`. Evaluate it
    // and dispatch the resulting value. Without this, `(lambda x: x)(5)`,
    // `functools.partial(f)(x)`, and `operator.itemgetter(1)(seq)` all fell
    // through to the name-based lookup with an empty name and raised NameError.
    if func_name.is_none() && !is_method_call {
        let callable = eval_expr(state, &node.func, tools).await?;
        let callable = crate::eval::functions::resolve_proxy(&callable).await?;
        let resolved_args = resolve_method_args(&args).await?;
        return call_value_as_function(state, &callable, &resolved_args, &kwargs, tools).await;
    }

    let name = func_name.as_deref().unwrap_or("");

    // 1. Tool dispatch — short-circuits on builtins so a host-registered tool named e.g. `print`
    //    cannot shadow the interpreter's own builtin. Delegates to
    //    `tools::resolver::resolve_and_dispatch` so the tool-resolution logic stays isolated and
    //    testable.
    if let Some(value) = crate::tools::resolver::resolve_and_dispatch(
        state,
        crate::tools::resolver::ToolCallDescriptor { name, args: &args, kwargs: &kwargs },
        tools,
    )
    .await?
    {
        return Ok(value);
    }

    // 2. Check builtins
    if let Some(result) = try_builtin(state, name, &args, &kwargs, tools).await? {
        return Ok(result);
    }

    // 4. Check state variables (user-defined functions / lambdas)
    let func_val = state.get_variable(name).cloned();
    if let Some(func_val) = func_val {
        match func_val {
            Value::Function(ref func_def) => {
                return call_user_function(state, func_def, &args, &kwargs, tools).await;
            }
            Value::Lambda(ref lambda_def) => {
                return call_lambda(state, lambda_def, &args, &kwargs, tools).await;
            }
            // Calling a class object instantiates it.
            Value::Class(ref class_name) => {
                return crate::eval::classes::instantiate(state, class_name, &args, &kwargs, tools)
                    .await;
            }
            // A name pulled in via `from module import func` (e.g. `sqrt`).
            Value::ModuleFunction { ref module, name: ref func } => {
                let module_name = module.clone();
                let func_name = func.clone();
                return crate::eval::modules::call_function(
                    state,
                    &module_name,
                    &func_name,
                    &args,
                    &kwargs,
                    tools,
                )
                .await;
            }
            // Everything else — BoundMethod, BuiltinTypeMethod, the
            // `__builtin__`/`__tool__`/`__class_method__` sentinel
            // strings — funnel through `call_value_as_function` so
            // every call surface uses the same dispatch table. The
            // direct-call name-lookup path used to error "'name' is
            // not callable" here, which was the bug that left
            // `fn = d.get; fn('A')` and `f = int; f("42")` broken
            // even after BoundMethod landed.
            ref other => {
                return call_value_as_function(state, other, &args, &kwargs, tools).await;
            }
        }
    }

    // 5. Check if it's an exception type constructor. With the
    // ExceptionType variant in play, indirect calls
    // (`E = ValueError; E("msg")`) route through call_value_as_function;
    // this arm covers the direct-call form where `name` is the raw
    // identifier from the AST. Args are preserved for `e.args`.
    if is_exception_type_name(name) {
        return crate::eval::exceptions::construct_exception_type(name, &args);
    }

    // `NameError`'s Display already renders `name '{0}' is not defined`, so the
    // variant payload is the bare identifier — passing a pre-formatted sentence
    // here double-wraps it into `name 'name '…' is not defined' is not defined`.
    Err(InterpreterError::name_not_defined(name).into())
}

/// Async `list.count` / `list.index` / `list.remove` using user-class
/// `__eq__` when needed. Returns `Ok(None)` if the receiver is not a
/// list (caller falls through to the positional method table).
async fn list_eq_method(
    state: &mut InterpreterState,
    obj_expr: &Expr,
    method_name: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> Result<Option<Value>, EvalError> {
    use crate::eval::functions::{reject_kwargs, sequence_index_range, to_len_i64};

    let raw_place = place::eval_place(state, obj_expr, tools).await?;
    let usable_place =
        raw_place.filter(|p| p.is_navigable() && state.variables.contains_key(&p.root));

    // Snapshot list items under the place lock (or from a temporary).
    let items: Vec<Value> = if let Some(pl) = &usable_place {
        let root = state
            .variables
            .get_mut(&pl.root)
            .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&pl.root)))?;
        let got = place::with_navigate_mut(root, &pl.steps, |target| {
            let Value::List(items) = target else {
                return Ok::<Option<Vec<Value>>, EvalError>(None);
            };
            Ok(Some(items.lock().clone()))
        })??;
        let Some(v) = got else {
            return Ok(None);
        };
        v
    } else {
        let obj = eval_expr(state, obj_expr, tools).await?;
        let Value::List(items) = obj else {
            return Ok(None);
        };
        match std::sync::Arc::try_unwrap(items) {
            Ok(mutex) => mutex.into_inner(),
            Err(shared) => shared.lock().clone(),
        }
    };

    // Arity / kwargs are validated only now that the receiver is
    // confirmed to be a list, so a non-list receiver of the same method
    // name (e.g. `itertools.count()`) already returned `None` above and
    // reaches its own dispatch instead of erroring here.
    reject_kwargs(method_name, kwargs)?;
    let needle = args.first().ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!(
            "{method_name}() takes exactly one argument (0 given)"
        )))
    })?;

    // `index` searches only the `[start, stop)` window; `count`/`remove` scan
    // the whole list and take exactly the needle.
    let (start, end) = if method_name == "index" {
        sequence_index_range(method_name, args, items.len())?
    } else {
        if args.len() != 1 {
            return Err(InterpreterError::TypeError(format!(
                "{method_name}() takes exactly one argument ({} given)",
                args.len()
            ))
            .into());
        }
        (0, items.len())
    };
    // Find first equal index (shared by index/remove).
    let mut first_eq: Option<usize> = None;
    let mut count = 0i64;
    for (i, item) in items.iter().enumerate().take(end).skip(start) {
        if crate::eval::op::eq(state, item, needle, tools).await? {
            count = count.saturating_add(1);
            if first_eq.is_none() {
                first_eq = Some(i);
            }
        }
    }

    match method_name {
        "count" => Ok(Some(Value::Int(count))),
        "index" => match first_eq {
            Some(i) => Ok(Some(Value::Int(to_len_i64(i)?))),
            None => Err(EvalError::Exception(ExceptionValue::new(
                "ValueError",
                format!("{} is not in list", needle.repr()),
            ))),
        },
        "remove" => {
            let Some(idx) = first_eq else {
                return Err(EvalError::Exception(ExceptionValue::new(
                    "ValueError",
                    "list.remove(x): x not in list",
                )));
            };
            if let Some(pl) = usable_place {
                let root = state
                    .variables
                    .get_mut(&pl.root)
                    .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&pl.root)))?;
                let freed = place::with_navigate_mut(root, &pl.steps, |target| {
                    let Value::List(items) = target else {
                        return Ok::<usize, EvalError>(0);
                    };
                    let removed = items.lock().remove(idx);
                    Ok(estimate_value_size(&removed))
                })??;
                place::apply_mem_delta(state, -place::to_isize(freed))?;
            }
            Ok(Some(Value::None))
        }
        _ => Ok(None),
    }
}

/// Extract function name and method call info from a Call func expression.
fn resolve_func_info(func_expr: &Expr) -> (Option<String>, bool, Option<&Expr>) {
    match func_expr {
        Expr::Name(name_node) => (Some(name_node.id.as_str().to_string()), false, None),
        Expr::Attribute(attr_node) => {
            (Some(attr_node.attr.as_str().to_string()), true, Some(attr_node.value.as_ref()))
        }
        _ => (None, false, None),
    }
}
