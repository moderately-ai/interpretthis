// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use rustpython_parser::ast;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{eval_expr, functions::resolve_proxy},
    security::validator,
    state::InterpreterState,
    tools::Tools,
    value::{ExceptionValue, Value, shared_list},
};

/// Evaluate a name reference (variable lookup).
pub fn eval_name(state: &InterpreterState, node: &ast::ExprName, tools: &Tools) -> EvalResult {
    let name = node.id.as_str();

    validator::validate_name(validator::NameContext::Access, name)?;

    // Check state variables
    if let Some(val) = state.get_variable(name) {
        return Ok(val.clone());
    }

    // Check tools
    if tools.contains_key(name) {
        // Return a sentinel that the call evaluator can recognize.
        // For name resolution, tools are not directly representable as Values,
        // so we store them as strings that the call path can look up.
        return Ok(Value::ToolName(name.to_string()));
    }

    // Python builtins that are always available as names:
    // True, False, None are handled as constants by the parser.
    if name == "NotImplemented" {
        return Ok(Value::NotImplemented);
    }

    // Builtin function names — these are handled by the call evaluator,
    // but we need to make them resolvable as names (for isinstance, callable checks etc.)
    let builtin_functions = [
        "print",
        "len",
        "range",
        "str",
        "int",
        "float",
        "complex",
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
        "iter",
        "next",
        "filter",
        "map",
        "repr",
        "hash",
        "id",
        "input",
        "pow",
        "divmod",
        "format",
        "object",
        "bin",
        "oct",
        "hex",
        "bytes",
        "bytearray",
    ];
    if builtin_functions.contains(&name) {
        return Ok(Value::BuiltinName(name.to_string()));
    }

    // Exception type names are valid names for raise/except.
    let exception_types = [
        "Exception",
        "ValueError",
        "TypeError",
        "KeyError",
        "IndexError",
        "AttributeError",
        "RuntimeError",
        "StopIteration",
        "ZeroDivisionError",
        "OverflowError",
        "AssertionError",
        "NotImplementedError",
        "FileNotFoundError",
        "IOError",
        "OSError",
        "NameError",
        "ArithmeticError",
        "LookupError",
        "ExceptionGroup",
        "BaseExceptionGroup",
    ];
    if exception_types.contains(&name) {
        return Ok(Value::ExceptionType(name.to_string()));
    }

    // Auto-imported modules (json, re, datetime) resolve without an explicit
    // `import`. Modules requiring an import (math, collections, …) are not here,
    // so they remain a NameError until imported — matching CPython.
    if crate::eval::modules::is_auto_imported(name) {
        return Ok(Value::Module(name.to_string()));
    }

    Err(InterpreterError::name_not_defined(name).into())
}

/// Evaluate a named expression (walrus operator) `(target := value)`.
///
/// Assigns `value` to `target` — always a bare name in valid Python syntax —
/// and returns the value so the enclosing expression (the `if` test, the
/// comprehension filter, etc.) can use it.
pub async fn eval_named_expr(
    state: &mut InterpreterState,
    node: &ast::ExprNamedExpr,
    tools: &Tools,
) -> EvalResult {
    let value = eval_expr(state, &node.value, tools).await?;
    match node.target.as_ref() {
        ast::Expr::Name(name_node) => {
            let name = name_node.id.as_str();
            validator::validate_name(validator::NameContext::Assignment, name)?;
            state.set_variable(name, value.clone()).map_err(EvalError::Interpreter)?;
            Ok(value)
        }
        other => Err(InterpreterError::Runtime(format!(
            "walrus assignment target must be a name, not {:?}",
            std::mem::discriminant(other)
        ))
        .into()),
    }
}

/// Evaluate attribute access (obj.attr).
///
/// Routes through `types::dispatch_getattr_opt` first — that covers
/// every builtin type with a fixed attribute table (dict, str, list,
/// set, tuple, plus the typed no-attr branches for none/bool/int/float
/// /bytes/range). State-dependent variants
/// (Instance/Class/Type/Function/Lambda/Module/Date/Exception) fall
/// through to `legacy_attribute` which owns the `&InterpreterState`
/// borrow for class-registry lookups. B1's user-class TypeObject
/// promotion replaces that fallback with a state-aware slot.
pub async fn eval_attribute(
    state: &mut InterpreterState,
    node: &ast::ExprAttribute,
    tools: &Tools,
) -> EvalResult {
    let attr_name = node.attr.as_str();
    validator::validate_attribute(attr_name)?;

    // Compute a place reference for the receiver expression first.
    // `eval_place` runs any index sub-expressions exactly once and
    // yields the navigable address of a mutable slot. We use it for
    // two purposes: (1) avoid double-evaluating side-effecting index
    // expressions when we still need the receiver's value below, and
    // (2) upgrade a Snapshot BoundMethod to a Place reference so
    // mutating methods captured into a variable (`push = xs.append`)
    // propagate back to the original variable.
    //
    // Navigation can fail when the receiver type doesn't model the
    // step kind in the slot model (`Class.attr`, `Module.member`,
    // `Type.attr` all fall through `value_attr_mut`'s catch-all). In
    // that case we drop back to `eval_expr` for the receiver value
    // AND suppress the Place upgrade — otherwise we'd construct a
    // BoundMethod whose Place can't be navigated at call time either.
    let place_opt = crate::eval::place::eval_place(state, &node.value, tools).await?;
    let usable_place: Option<&crate::eval::place::Place> = match &place_opt {
        Some(p) if p.is_navigable() && state.variables.contains_key(&p.root) => Some(p),
        _ => None,
    };

    let nav_value: Option<Value> = match usable_place {
        Some(place) => {
            let mut root = state
                .get_variable(&place.root)
                .cloned()
                .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&place.root)))?;
            crate::eval::place::with_navigate_mut(&mut root, &place.steps, |target| target.clone())
                .ok()
        }
        None => None,
    };
    // place_for_upgrade is Some ONLY when navigation succeeded. The
    // Place we'd attach to a BoundMethod must navigate cleanly at
    // call time, so a place that failed peek-navigation here can't
    // be trusted later.
    let (obj, place_for_upgrade) = match nav_value {
        Some(v) => (v, usable_place),
        None => (eval_expr(state, &node.value, tools).await?, None),
    };
    let obj = resolve_proxy(&obj).await?;
    getattr_on_value(state, obj, attr_name, tools, place_for_upgrade).await
}

/// Attribute lookup on an already-evaluated receiver. Shared by
/// `obj.attr` and the bounded `getattr` builtin. Always runs
/// [`validate_attribute`] so blocked dunders stay unreachable.
pub(crate) async fn getattr_on_value(
    state: &mut InterpreterState,
    obj: Value,
    attr_name: &str,
    tools: &Tools,
    place_for_upgrade: Option<&crate::eval::place::Place>,
) -> EvalResult {
    validator::validate_attribute(attr_name)?;

    // Property dispatch (B2): if `obj` is an instance whose MRO
    // defines a @property for this attribute, call the getter. Data
    // descriptors beat instance dict in CPython's lookup order.
    if let Value::Instance(inst) = &obj {
        if let Some(prop) =
            crate::eval::classes::lookup_property(state, &inst.class_name, attr_name)
        {
            return crate::eval::classes::invoke_property_getter(
                state,
                &prop.getter,
                obj.clone(),
                tools,
            )
            .await;
        }
        // User descriptors on class attrs. CPython precedence:
        // data descriptor (__set__ or __delete__) > instance dict >
        // non-data descriptor (__get__ only).
        if let Some(desc) =
            crate::eval::classes::lookup_class_attr_instance(state, &inst.class_name, attr_name)
        {
            let has_get =
                crate::eval::classes::lookup_method_in_mro(state, &desc.class_name, "__get__")
                    .is_some();
            if has_get {
                let is_data =
                    crate::eval::classes::lookup_method_in_mro(state, &desc.class_name, "__set__")
                        .is_some()
                        || crate::eval::classes::lookup_method_in_mro(
                            state,
                            &desc.class_name,
                            "__delete__",
                        )
                        .is_some();
                if !is_data {
                    // Non-data: instance field wins when present.
                    if let Some(v) = inst.fields.lock().get(attr_name) {
                        return Ok(v.clone());
                    }
                }
                if let Some((_, get_method)) =
                    crate::eval::classes::lookup_method_in_mro(state, &desc.class_name, "__get__")
                {
                    let owner = Value::Class(inst.class_name.clone());
                    let call = crate::eval::functions::CallArgs {
                        positional: &[obj.clone(), owner],
                        keyword: &indexmap::IndexMap::new(),
                    };
                    let (returned, _) = crate::eval::classes::call_method(
                        state,
                        &get_method,
                        Value::Instance(desc),
                        call,
                        tools,
                    )
                    .await?;
                    return Ok(returned);
                }
            }
        }
    }

    // Type-as-receiver: `str.upper`, `int.bit_length`, `list.append`,
    // etc. The bare type name resolves to `Value::BuiltinName`; an
    // attribute on it is the unbound method descriptor, NOT an instance
    // method bound to a string. Without this intercept, `str.upper`
    // would fall into the dispatch_getattr_opt below — but BuiltinName
    // has no get-attr slot, so the lookup would error.
    if let Value::BuiltinName(type_name) = &obj {
        return Ok(Value::BuiltinTypeMethod {
            type_name: type_name.clone(),
            method: attr_name.to_string(),
        });
    }

    if let Some(val) = crate::types::dispatch_getattr_opt(&obj, attr_name)? {
        // Upgrade Snapshot → Place when the receiver expression was a
        // navigable place. Mutations through the captured bound
        // method then navigate back to the original variable instead
        // of mutating a discarded clone.
        if let (
            Value::BoundMethod { receiver: crate::value::BoundMethodReceiver::Snapshot(_), method },
            Some(place),
        ) = (&val, place_for_upgrade)
        {
            let bm_steps: Vec<crate::value::BoundMethodStep> = place
                .steps
                .iter()
                .filter_map(|s| match s {
                    crate::eval::place::PlaceStep::Index(v) => {
                        Some(crate::value::BoundMethodStep::Index(v.clone()))
                    }
                    crate::eval::place::PlaceStep::Attr(n) => {
                        Some(crate::value::BoundMethodStep::Attr(n.clone()))
                    }
                    // `is_navigable` filtered out Slice steps above.
                    crate::eval::place::PlaceStep::Slice(_) => None,
                })
                .collect();
            return Ok(Value::BoundMethod {
                receiver: crate::value::BoundMethodReceiver::Place {
                    root: place.root.clone(),
                    steps: bm_steps,
                },
                method: method.clone(),
            });
        }
        return Ok(val);
    }
    match legacy_attribute(state, &obj, attr_name) {
        Ok(v) => Ok(v),
        Err(err) => {
            // `__getattr__` fires only on a miss (AttributeError).
            // Other errors propagate unchanged.
            let is_attribute_error =
                matches!(err, EvalError::Interpreter(InterpreterError::AttributeError(_)));
            if !is_attribute_error {
                return Err(err);
            }
            if let Value::Instance(inst) = &obj {
                if let Some((_, method)) = crate::eval::classes::lookup_method_in_mro(
                    state,
                    &inst.class_name,
                    "__getattr__",
                ) {
                    let attr_arg = Value::String(attr_name.into());
                    let call = crate::eval::functions::CallArgs {
                        positional: std::slice::from_ref(&attr_arg),
                        keyword: &indexmap::IndexMap::new(),
                    };
                    let (returned, _self) =
                        crate::eval::classes::call_method(state, &method, obj.clone(), call, tools)
                            .await?;
                    return Ok(returned);
                }
            }
            Err(err)
        }
    }
}

/// State-aware attribute fallback for variants that consult
/// `InterpreterState`'s class registry (Instance/Class) or carry
/// type-specific attribute metadata (Type/Function/Lambda/Module/Date
/// /Exception). Stays here rather than in `types.rs` until B1 promotes
/// the user-class TypeObject and the slot signature gains a state arg.
fn legacy_attribute(state: &InterpreterState, obj: &Value, attr_name: &str) -> EvalResult {
    match obj {
        Value::Exception(exc) => exception_attribute(exc, attr_name),
        Value::Instance(inst) => crate::eval::classes::instance_attribute(state, inst, attr_name),
        Value::Class(class_name) => {
            crate::eval::classes::class_attribute(state, class_name, attr_name)
        }
        Value::Type(type_name) | Value::ExceptionType(type_name) => {
            if attr_name == "__name__" || attr_name == "__qualname__" {
                Ok(Value::String(type_name.clone().into()))
            } else {
                Err(attribute_error("type", attr_name))
            }
        }
        Value::Function(func_def) => {
            if attr_name == "__name__" || attr_name == "__qualname__" {
                Ok(Value::String(func_def.name.clone().into()))
            } else {
                Err(attribute_error("function", attr_name))
            }
        }
        Value::Lambda(_) => {
            if attr_name == "__name__" || attr_name == "__qualname__" {
                Ok(Value::String("<lambda>".into()))
            } else {
                Err(attribute_error("function", attr_name))
            }
        }
        Value::Module(module) => crate::eval::modules::module_member(module, attr_name),
        // Constructor classmethods: `f = datetime.strptime` (no call yet).
        // Live calls `datetime.strptime(...)` are handled in eval_call's
        // method path via the same registry — keep both in sync.
        Value::ModuleFunction { module, name } => {
            if let Some(func) = crate::eval::modules::type_classmethod(module, name, attr_name) {
                Ok(Value::ModuleFunction { module: module.clone(), name: func.into() })
            } else {
                Err(attribute_error(obj.type_name(), attr_name))
            }
        }
        _ => Err(attribute_error(obj.type_name(), attr_name)),
    }
}

/// Build an `AttributeError` for `'<type>' object has no attribute '<attr>'`.
fn attribute_error(type_name: &str, attr_name: &str) -> EvalError {
    InterpreterError::AttributeError(format!("'{type_name}' object has no attribute '{attr_name}'"))
        .into()
}

/// Exception attribute access. CPython attributes exposed:
///
/// - `.args` — tuple of constructor positional args. Synthesised as `(message,)` when the exception
///   came from an internal raise (KeyError, IndexError, etc.) that didn't populate `args`
///   explicitly; user-constructed exceptions (`ValueError('a','b')`) carry the exact args.
/// - `.__cause__` — the chained `raise X from Y` cause; `None` if not set. Returns a
///   `Value::Exception` wrapping the cause so user code can walk the chain.
/// - `.__context__` — same as `__cause__` in our model (we don't distinguish implicit context from
///   explicit cause).
/// - `.message` — legacy CPython 2 alias for backward compat with code that hasn't migrated; just
///   the message body.
fn exception_attribute(exc: &ExceptionValue, attr_name: &str) -> EvalResult {
    match attr_name {
        "exceptions" => {
            let items = exc
                .exceptions
                .as_ref()
                .map(|xs| xs.iter().cloned().map(|e| Value::Exception(Box::new(e))).collect())
                .unwrap_or_default();
            Ok(Value::Tuple(items))
        }
        "subgroup" | "split" => Ok(Value::ExceptionMethod {
            method: attr_name.to_string(),
            exception: Box::new(exc.clone()),
        }),
        // `args` is the truth — ExceptionValue::new defaults it from
        // the message (empty message → empty tuple, non-empty →
        // (message,)) so this never needs a synthesis fallback. Multi-
        // arg constructors override via with_args at construction.
        "args" => Ok(Value::Tuple(exc.args.clone())),
        "__cause__" | "__context__" => {
            Ok(exc.cause.as_ref().map_or(Value::None, |cause| Value::Exception(cause.clone())))
        }
        "message" => Ok(Value::String(exc.message.clone().into())),
        _ => Err(attribute_error(&exc.type_name, attr_name)),
    }
}

/// Evaluate subscript access (obj[key]).
///
/// Slices (`a[1:10:2]`) keep the per-type path here — they're a uniform
/// sequence operation that builds a fresh container, not a per-item
/// `__getitem__`. Index access routes through `dispatch_getitem` (A5) so
/// list/tuple/str/bytes/dict/range all share one entry point.
pub async fn eval_subscript(
    state: &mut InterpreterState,
    node: &ast::ExprSubscript,
    tools: &Tools,
) -> EvalResult {
    // Bare-Name receiver fast path. Today `eval_expr(&node.value)` for
    // `Expr::Name("d")` clones the entire container (Value::Dict with N
    // entries → N value clones + IndexMap clone) just so the subsequent
    // dispatch_getitem can look up one slot and clone its value. For a
    // 100-entry dict accessed in a tight loop (the canonical
    // `dict_get_in_loop_10k` shape) that's ~3 µs of dict-clone per
    // iteration vs ~100 ns for the actual lookup.
    //
    // The fast path borrows the receiver directly from `state.variables`
    // and runs `dispatch_getitem` against the borrow. It's restricted to
    // shapes that preserve CPython's left-to-right evaluation order:
    //
    // - receiver is a bare `Expr::Name` (zero side effects in eval)
    // - slice is `Expr::Constant` OR an `Expr::Name` referring to a different binding (no side
    //   effects, no reassignment of the container between container-eval and index-eval)
    // - slice is NOT a Slice node (those keep the existing slice path)
    // - the bound value is one of the builtin containers that have a sync `get_item_slot` (Dict /
    //   List / Tuple / String / Range / Bytes) — Instance / DefaultDict / namedtuple keep the
    //   existing async path so their state-aware intercepts run.
    //
    // Any expression failing these gates falls through to the original
    // clone-then-dispatch path.
    if let ast::Expr::Name(name_node) = node.value.as_ref() {
        if !matches!(node.slice.as_ref(), ast::Expr::Slice(_)) {
            let container_name = name_node.id.as_str();
            let slice_is_static = match node.slice.as_ref() {
                ast::Expr::Constant(_) => true,
                ast::Expr::Name(slice_name) => slice_name.id.as_str() != container_name,
                _ => false,
            };
            if slice_is_static {
                let index = match crate::eval::try_eval_expr_sync(state, &node.slice, tools) {
                    Some(r) => r?,
                    None => eval_expr(state, &node.slice, tools).await?,
                };
                if let Some(container) = state.variables.get(container_name) {
                    let take_fast_path = matches!(
                        container,
                        Value::Dict(_)
                            | Value::List(_)
                            | Value::Tuple(_)
                            | Value::String(_)
                            | Value::Range { .. }
                            | Value::Bytes(_)
                    );
                    // Instance keys need async `__hash__` / `__eq__` via
                    // `op::getitem`. The sync `dispatch_getitem` path rejects
                    // them as unhashable (`type_name() == "object"`).
                    if take_fast_path && !matches!(index, Value::Instance(_)) {
                        return crate::types::dispatch_getitem(container, &index);
                    }
                }
            }
        }
    }

    let obj = eval_expr(state, &node.value, tools).await?;
    let obj = resolve_proxy(&obj).await?;

    // Check if the slice is an actual Slice node
    if let ast::Expr::Slice(slice_node) = node.slice.as_ref() {
        return eval_subscript_slice(state, &obj, slice_node, tools).await;
    }

    let index = eval_expr(state, &node.slice, tools).await?;

    // namedtuple subscript intercept: `nt[0]` returns the value of the
    // 0-th declared field. Instances don't have a get_item_slot, so
    // without this they'd raise TypeError. The class's `_fields`
    // tuple (set at namedtuple-synthesis time) carries the
    // declaration order; positive and negative ints are accepted.
    if let Value::Instance(inst) = &obj {
        if let Some(Value::Tuple(field_names)) =
            state.classes.get(&inst.class_name).and_then(|c| c.class_attrs.get("_fields"))
        {
            if let Value::Int(i) = &index {
                let len = field_names.len();
                let idx = if *i < 0 {
                    usize::try_from(i64::try_from(len).unwrap_or(i64::MAX) + *i).ok()
                } else {
                    usize::try_from(*i).ok()
                };
                if let Some(idx) = idx.filter(|&n| n < len) {
                    if let Value::String(field_name) = &field_names[idx] {
                        return Ok(inst
                            .fields
                            .lock()
                            .get(field_name.as_str())
                            .cloned()
                            .unwrap_or(Value::None));
                    }
                }
                return Err(InterpreterError::Runtime(format!(
                    "tuple index out of range: {i} (len {len})"
                ))
                .into());
            }
        }
    }

    // defaultdict missing-key intercept: if `obj` is a DefaultDict
    // and the key is absent, invoke the factory and insert the
    // result. The sync `dispatch_getitem` slot can't invoke a user
    // factory function; the intercept happens here so the factory
    // call gets `state` + `tools` + async. Routed before `op::getitem`
    // because the defaultdict factory is a stored callable, not a
    // class slot — the dunder-dispatch path would skip it.
    if let Value::DefaultDict(data) = &obj {
        let key = crate::eval::literals::value_to_key(&index)?;
        if let Some(value) = data.items.get(&key) {
            return Ok(value.clone());
        }
        // Materialise via factory.
        let synthesised = invoke_factory(state, &data.factory, tools).await?;
        // Write back: the DefaultDict variable, if obj was rooted at
        // a name, gets updated. For non-place receivers (literal /
        // function-call result) the synthesis is discarded after
        // this read — matches CPython where d[k] always mutates,
        // but our owned model can't propagate the mutation through
        // a literal.
        if let ast::Expr::Name(name_node) = node.value.as_ref() {
            let name = name_node.id.as_str().to_string();
            if let Some(Value::DefaultDict(data)) = state.variables.get(&name).cloned() {
                let mut new_data = *data;
                new_data.items.insert(key, synthesised.clone());
                state
                    .set_variable(&name, Value::DefaultDict(Box::new(new_data)))
                    .map_err(EvalError::Interpreter)?;
            }
        }
        return Ok(synthesised);
    }

    crate::eval::op::getitem(state, &obj, &index, tools).await
}

/// Public re-export of `invoke_factory` so the aug-assign pre-touch
/// in eval/statements.rs can synthesise defaultdict entries before
/// the place machinery navigates.
pub async fn invoke_factory_pub(
    state: &mut InterpreterState,
    factory: &Value,
    tools: &Tools,
) -> EvalResult {
    invoke_factory(state, factory, tools).await
}

/// Call the defaultdict factory to synthesise a value for a missing
/// key. Accepts Function / Lambda / Class plus the builtin sentinels
/// `__builtin__int`/`list`/`dict`/`set`/`tuple`/`str`/`float`/`bool`,
/// since defaultdict(int) is the most common form in user code.
async fn invoke_factory(
    state: &mut InterpreterState,
    factory: &Value,
    tools: &Tools,
) -> EvalResult {
    let kwargs: indexmap::IndexMap<String, Value> = indexmap::IndexMap::new();
    let empty: [Value; 0] = [];
    match factory {
        Value::Function(def) => {
            crate::eval::functions::call_user_function(state, def, &empty, &kwargs, tools).await
        }
        Value::Lambda(def) => {
            crate::eval::functions::call_lambda(state, def, &empty, &kwargs, tools).await
        }
        Value::Class(name) => {
            crate::eval::classes::instantiate(state, name, &empty, &kwargs, tools).await
        }
        Value::None => Ok(Value::None),
        Value::BuiltinName(builtin) => {
            // Invoke the builtin constructor by name with no args.
            match builtin.as_str() {
                "int" => Ok(Value::Int(0)),
                "float" => Ok(Value::Float(0.0)),
                "bool" => Ok(Value::Bool(false)),
                "str" => Ok(Value::String("".into())),
                "bytes" => Ok(Value::Bytes(Vec::new())),
                "list" => Ok(Value::List(shared_list(Vec::new()))),
                "tuple" => Ok(Value::Tuple(Vec::new())),
                "dict" => Ok(Value::Dict(indexmap::IndexMap::new())),
                "set" => Ok(Value::Set(Vec::new())),
                _ => Err(InterpreterError::TypeError(format!(
                    "defaultdict factory builtin '{builtin}' is not zero-arg constructable"
                ))
                .into()),
            }
        }
        other => Err(InterpreterError::TypeError(format!(
            "defaultdict factory must be callable (got '{}')",
            other.type_name()
        ))
        .into()),
    }
}

/// Evaluate a subscript with a slice (obj[start:stop:step]).
async fn eval_subscript_slice(
    state: &mut InterpreterState,
    obj: &Value,
    slice_node: &ast::ExprSlice,
    tools: &Tools,
) -> EvalResult {
    let lower = if let Some(ref expr) = slice_node.lower {
        Some(eval_expr(state, expr, tools).await?)
    } else {
        None
    };
    let upper = if let Some(ref expr) = slice_node.upper {
        Some(eval_expr(state, expr, tools).await?)
    } else {
        None
    };
    let step_expr = if let Some(ref expr) = slice_node.step {
        Some(eval_expr(state, expr, tools).await?)
    } else {
        None
    };

    let stride = match &step_expr {
        Some(Value::Int(s)) => {
            if *s == 0 {
                return Err(InterpreterError::ValueError("slice step cannot be zero".into()).into());
            }
            *s
        }
        Some(Value::None) | None => 1,
        Some(other) => {
            return Err(InterpreterError::TypeError(format!(
                "slice indices must be integers or None, not '{}'",
                other.type_name()
            ))
            .into());
        }
    };

    match obj {
        Value::List(items) => {
            let snapshot = items.lock().clone();
            let sliced = slice_sequence(&snapshot, lower.as_ref(), upper.as_ref(), stride)?;
            Ok(Value::List(shared_list(sliced)))
        }
        Value::Tuple(items) => {
            let sliced = slice_sequence(items, lower.as_ref(), upper.as_ref(), stride)?;
            Ok(Value::Tuple(sliced))
        }
        Value::String(s) => {
            let chars: Vec<Value> =
                s.chars().map(|c| Value::String(c.to_string().into())).collect();
            let sliced = slice_sequence(&chars, lower.as_ref(), upper.as_ref(), stride)?;
            let result: String = sliced
                .into_iter()
                .map(|v| match v {
                    Value::String(s) => s.into(),
                    _ => String::new(),
                })
                .collect();
            Ok(Value::String(result.into()))
        }
        Value::Bytes(b) => {
            // Bytes slice — same shape as a string slice but emitting
            // bytes. Each byte becomes a Value::Int for the shared
            // slice_sequence helper, then the result collapses back
            // into Value::Bytes.
            let elems: Vec<Value> = b.iter().map(|&n| Value::Int(i64::from(n))).collect();
            let sliced = slice_sequence(&elems, lower.as_ref(), upper.as_ref(), stride)?;
            let bytes: Vec<u8> = sliced
                .into_iter()
                .filter_map(|v| match v {
                    Value::Int(n) => u8::try_from(n & 0xFF).ok(),
                    _ => None,
                })
                .collect();
            Ok(Value::Bytes(bytes))
        }
        _ => Err(InterpreterError::TypeError(format!(
            "'{}' object is not subscriptable",
            obj.type_name()
        ))
        .into()),
    }
}

/// Slice a sequence (list, tuple, or string chars) with Python slice semantics.
fn slice_sequence(
    items: &[Value],
    lower: Option<&Value>,
    upper: Option<&Value>,
    stride: i64,
) -> Result<Vec<Value>, EvalError> {
    let len = i64::try_from(items.len()).map_err(|_| {
        InterpreterError::Runtime("sequence length overflows i64 for slicing".into())
    })?;

    let resolve_index = |val: Option<&Value>, default: i64| -> Result<i64, EvalError> {
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
    };

    // Helper to pull a positive-sense index out for element access. The caller
    // loop keeps `i` inside [0, len), so the try_from cannot fail in practice;
    // we use try_from rather than `as` so a violation surfaces as a clean
    // internal error instead of a silent wrap.
    let to_index = |i: i64| -> Result<usize, EvalError> {
        usize::try_from(i).map_err(|_| {
            InterpreterError::Runtime("slice index overflow (internal invariant)".into()).into()
        })
    };

    if stride > 0 {
        let raw_start = resolve_index(lower, 0)?;
        let raw_stop = resolve_index(upper, len)?;

        let begin = clamp_slice_index(raw_start, len);
        let end = clamp_slice_index(raw_stop, len);

        let mut result = Vec::new();
        let mut i = begin;
        while i < end {
            result.push(items[to_index(i)?].clone());
            i += stride;
        }
        Ok(result)
    } else {
        // Negative stride: iterate backwards. `clamp_slice_index_neg` returns
        // values in [-1, len - 1]; the loop condition `i > end` keeps `i`
        // strictly positive when it's used to index.
        let raw_start = resolve_index(lower, len - 1)?;
        let raw_stop = resolve_index(upper, -(len + 1))?;

        let begin = clamp_slice_index_neg(raw_start, len);
        let end = clamp_slice_index_neg(raw_stop, len);

        let mut result = Vec::new();
        let mut i = begin;
        while i > end {
            result.push(items[to_index(i)?].clone());
            i += stride;
        }
        Ok(result)
    }
}

/// Clamp a slice index for positive step.
pub(crate) fn clamp_slice_index(idx: i64, len: i64) -> i64 {
    let adjusted = if idx < 0 { idx + len } else { idx };
    adjusted.max(0).min(len)
}

/// Clamp a slice index for negative step.
pub(crate) fn clamp_slice_index_neg(idx: i64, len: i64) -> i64 {
    let adjusted = if idx < 0 { idx + len } else { idx };
    adjusted.max(-1).min(len - 1)
}

/// Evaluate a Slice expression (produces a `Value::Tuple` with start/stop/step).
/// This is used for standalone slice evaluation; subscript slicing is handled inline.
pub async fn eval_slice(
    state: &mut InterpreterState,
    node: &ast::ExprSlice,
    tools: &Tools,
) -> EvalResult {
    let lower = if let Some(ref expr) = node.lower {
        eval_expr(state, expr, tools).await?
    } else {
        Value::None
    };
    let upper = if let Some(ref expr) = node.upper {
        eval_expr(state, expr, tools).await?
    } else {
        Value::None
    };
    let stride = if let Some(ref expr) = node.step {
        eval_expr(state, expr, tools).await?
    } else {
        Value::None
    };

    Ok(Value::Tuple(vec![lower, upper, stride]))
}
