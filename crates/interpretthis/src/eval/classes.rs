// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! User-defined classes: `class` definition, instantiation, and method calls.
//!
//! A class definition registers a [`ClassValue`] (its methods and class
//! attributes) in `InterpreterState::classes` and binds a lightweight
//! [`Value::Class`] handle as the class-named variable. Instances carry only
//! their class name plus their own fields; methods are looked up in the registry
//! at call time, so an instance never copies its class's methods.
//!
//! Because the value model is owned (no reference aliasing), a method that
//! mutates `self` mutates a *copy*. [`call_method`] therefore reads `self` back
//! out of the method's scope after the body runs and returns it, and the caller
//! writes it back into the receiver's slot — the same place-based write-back the
//! built-in mutating methods use.
//!
//! Inheritance: multi-level inheritance and the `super()` family are
//! supported via a C3-linearized MRO computed at class-definition time.
//! Method and attribute lookups walk `ClassValue::mro` in order. The
//! implicit `object` tail is omitted from the MRO (no registered class) —
//! lookups fall through to a not-found / AttributeError once the explicit
//! chain is exhausted, matching CPython's behaviour without modelling
//! `object`'s instance methods.
//!
//! Scope limits (rejected with a clear error rather than silently mis-handled):
//! metaclass / keyword arguments (out-of-scope) and inheritance cycles
//! or C3 conflicts (raise `TypeError("Cannot create a consistent method
//! resolution order")` matching CPython). Class-level decorators
//! (`@property`-style + `@dataclass` + user callables) are supported;
//! method-level decorators route through `classify_decorated_method`.

use std::{collections::BTreeMap, sync::Arc};

use indexmap::IndexMap;
use rustpython_parser::ast::{self, Expr, Stmt};

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{
        eval_expr,
        functions::{
            CallArgs, bind_params, build_function_params, call_lambda, call_user_function,
            execute_body, extract_function_source,
        },
    },
    state::InterpreterState,
    tools::Tools,
    value::{ClassValue, FunctionDef, InstanceValue, PropertyDef, Value, shared_list},
};

/// Evaluate a `class` definition: build the [`ClassValue`], register it, and
/// bind the class-named variable to a [`Value::Class`] handle.
pub async fn eval_class_def(
    state: &mut InterpreterState,
    node: &ast::StmtClassDef,
    tools: &Tools,
) -> EvalResult {
    let class_name = node.name.as_str();
    crate::security::validator::validate_name(
        crate::security::validator::NameContext::Assignment,
        class_name,
    )?;

    // Optional `metaclass=`; every other class keyword (`class C(Base,
    // prefix="x")`) is forwarded to `__init_subclass__` per PEP 487.
    let mut metaclass_name: Option<String> = None;
    let mut init_subclass_kwargs: IndexMap<String, Value> = IndexMap::new();
    for kw in &node.keywords {
        let key = kw.arg.as_ref().map(|a| a.as_str());
        let val = eval_expr(state, &kw.value, tools).await?;
        if key == Some("metaclass") {
            match val {
                Value::BuiltinName(n) | Value::Type(n) if n == "type" => {}
                Value::Class(n) => metaclass_name = Some(n),
                other => {
                    return Err(InterpreterError::TypeError(format!(
                        "metaclass must be a type, not '{}'",
                        other.type_name()
                    ))
                    .into());
                }
            }
        } else if let Some(name) = key {
            init_subclass_kwargs.insert(name.to_string(), val);
        }
    }

    // Collect declared bases (excluding the implicit `object`, which we
    // don't materialize as a registered class). typing.* and enum.*
    // aliases (`Enum`, `IntEnum`, `NamedTuple`, etc.) resolve to
    // Value::Type / Value::ModuleFunction sentinels and are
    // recognised here for enum-kind detection.
    let mut bases: Vec<String> = Vec::new();
    let mut enum_kind: Option<crate::value::EnumKind> = None;
    // `class P(NamedTuple):` — a typing.NamedTuple class becomes a namedtuple
    // built from its annotations (with defaults) after the body is processed.
    let mut is_typing_namedtuple = false;
    for base in &node.bases {
        // Resolve bare names from the environment, or evaluate computed
        // base expressions (`collections.UserList`, `mod.Base`, …).
        let (base_name, resolved): (String, Option<Value>) = match base {
            Expr::Name(name_node) => {
                let base_name = name_node.id.as_str().to_string();
                let resolved = state.variables.get(&base_name).cloned();
                (base_name, resolved)
            }
            other => {
                let val = eval_expr(state, other, tools).await?;
                match &val {
                    Value::Class(n) | Value::ExceptionType(n) => (n.clone(), Some(val)),
                    Value::Type(n) => (n.clone(), Some(val)),
                    other_v => {
                        return Err(InterpreterError::TypeError(format!(
                            "bases must be types, not '{}'",
                            other_v.type_name()
                        ))
                        .into());
                    }
                }
            }
        };
        if base_name == "object" {
            continue;
        }
        // Check resolved value for the enum sentinels (`enum.Enum`,
        // `enum.IntEnum`, `enum.StrEnum`, etc.) so aliased imports
        // (`from enum import Enum as MyBase`) work too.
        if let Some(Value::Type(type_name)) = &resolved {
            match type_name.as_str() {
                "enum.Enum" => enum_kind = Some(crate::value::EnumKind::Plain),
                "enum.IntEnum" => enum_kind = Some(crate::value::EnumKind::Int),
                "enum.Flag" => enum_kind = Some(crate::value::EnumKind::Flag),
                "enum.IntFlag" => enum_kind = Some(crate::value::EnumKind::IntFlag),
                "enum.StrEnum" => enum_kind = Some(crate::value::EnumKind::Str),
                "typing.NamedTuple" => is_typing_namedtuple = true,
                // `abc.ABC`/`ABCMeta` are abstract-base markers with no
                // registered ClassValue — the abstract-method tracking below
                // makes the class abstract, so the sentinel itself is skipped.
                "abc.ABC" | "abc.ABCMeta" => {}
                _ => {}
            }
            continue;
        }
        if matches!(resolved, Some(Value::ModuleFunction { .. })) {
            continue;
        }
        // Builtin exception types (`Exception`, `ValueError`, …) are
        // not registered in `state.classes` — they resolve as
        // ExceptionType sentinels on bare-name lookup. Allow them as
        // bases so `class AppError(Exception)` works; MRO stores the
        // name string and except-matching walks it via the hard-coded
        // hierarchy + user MRO.
        let is_exception_base = crate::eval::functions::is_exception_type_name(&base_name)
            || matches!(resolved, Some(Value::ExceptionType(_)));
        if !state.classes.contains_key(&base_name) && !is_exception_base {
            return Err(InterpreterError::name_not_defined(&base_name).into());
        }
        bases.push(base_name);
    }

    let mut methods: BTreeMap<String, FunctionDef> = BTreeMap::new();
    let mut class_attrs: BTreeMap<String, Value> = BTreeMap::new();
    let mut properties: BTreeMap<String, PropertyDef> = BTreeMap::new();
    let mut static_methods: BTreeMap<String, FunctionDef> = BTreeMap::new();
    let mut class_methods: BTreeMap<String, FunctionDef> = BTreeMap::new();
    // Annotated attribute names in declaration order. `@dataclass` consumes
    // this to compute its field list — class_attrs (BTreeMap) would
    // re-order them alphabetically and break the synthesized `__init__`
    // parameter order.
    let mut annotations: Vec<String> = Vec::new();
    // Enum member names in declaration order, for order-preserving iteration.
    let mut enum_members: Vec<String> = Vec::new();
    // Next value an `auto()` member resolves to (see `wrap_enum_member`).
    let mut enum_auto_next: i64 = 1;
    // Method names decorated `@abstractmethod` in this class body.
    let mut abstract_here: Vec<String> = Vec::new();

    // PEP 3115: Meta.__prepare__(name, bases) may supply the initial namespace.
    if let Some(ref meta) = metaclass_name {
        if let Some(prepared) =
            invoke_metaclass_prepare(state, meta, class_name, &bases, tools).await?
        {
            for (k, v) in prepared {
                if let crate::value::ValueKey::String(s) = k {
                    class_attrs.insert(s.to_string(), v);
                }
            }
        }
    }

    // CPython executes a class body as a code block against its own
    // namespace: each statement can read names bound earlier in the body
    // (`x = 1; y = x + 1`), names bound inside nested if/for/while (loop
    // variables included) become class attributes, and reads fall through
    // to the enclosing scope. We model the namespace with `state.variables`,
    // seeding the class dict built so far before each statement and
    // harvesting freshly-bound names afterwards, then restore the enclosing
    // scope so nothing leaks out — even if the body raises.
    let class_scope_saved = state.variables.clone();
    let body_result: Result<(), EvalError> = async {
        for stmt in &node.body {
            // Make the class namespace built so far visible to this statement.
            for (name, value) in &class_attrs {
                state.variables.insert(name.clone(), value.clone());
            }
            match stmt {
                Stmt::FunctionDef(method) => {
                    let mut params = build_function_params(&method.args)?;
                    // Evaluate the method's default args at def time in
                    // the class body scope — same CPython semantics as
                    // top-level `def` (the `i=i` capture idiom + mutable
                    // default sharing).
                    crate::eval::functions::evaluate_param_defaults(state, &mut params, tools)
                        .await?;
                    let source = extract_function_source(&state.current_source, method);
                    let method_name = method.name.as_str().to_string();
                    // `@abstractmethod` (and its `abc.`-qualified / property
                    // variants) flags the method as abstract and is stripped
                    // before classification so the remaining decorators
                    // (property/classmethod/staticmethod/none) process normally.
                    let has_abstract =
                        method.decorator_list.iter().any(is_abstractmethod_decorator);
                    if has_abstract {
                        abstract_here.push(method_name.clone());
                    }
                    let decorators: Vec<Expr> = method
                        .decorator_list
                        .iter()
                        .filter(|d| !is_abstractmethod_decorator(d))
                        .cloned()
                        .collect();
                    // The body cache key gets a per-decorator suffix so
                    // `@property def x` / `@x.setter def x` / `@x.deleter
                    // def x` don't collide. Regular methods use the plain
                    // `Class.name` key.
                    classify_decorated_method(
                        state,
                        &decorators,
                        class_name,
                        method_name,
                        params,
                        method.body.clone(),
                        source,
                        &mut methods,
                        &mut properties,
                        &mut static_methods,
                        &mut class_methods,
                    )?;
                }
                Stmt::Assign(assign) => {
                    // Every target receives the value: chained (`a = b = 1`) and
                    // tuple/list unpacking (`X, Y = 1, 2`) both land as class attrs.
                    let value = eval_expr(state, &assign.value, tools).await?;
                    for target in &assign.targets {
                        bind_class_target(
                            target,
                            &value,
                            enum_kind,
                            class_name,
                            &mut class_attrs,
                            &mut enum_auto_next,
                        )?;
                        // Record enum member declaration order (simple-name
                        // targets that wrap into a member).
                        if enum_kind.is_some() {
                            if let Expr::Name(n) = target {
                                let member = n.id.as_str();
                                if matches!(class_attrs.get(member), Some(Value::EnumMember { .. }))
                                    && !enum_members.iter().any(|m| m == member)
                                {
                                    enum_members.push(member.to_string());
                                }
                            }
                        }
                    }
                }
                Stmt::AnnAssign(ann) => {
                    if let Expr::Name(target) = ann.target.as_ref() {
                        let attr_name = target.id.as_str().to_string();
                        // Record every annotated name, with or without a value,
                        // in declaration order so `@dataclass` can read fields.
                        if !annotations.contains(&attr_name) {
                            annotations.push(attr_name.clone());
                        }
                        if let Some(value_expr) = &ann.value {
                            let value = eval_expr(state, value_expr, tools).await?;
                            let wrapped = wrap_enum_member(
                                enum_kind,
                                class_name,
                                &attr_name,
                                value,
                                &mut enum_auto_next,
                            );
                            class_attrs.insert(attr_name, wrapped);
                        }
                    }
                }
                // Nested control flow / nested class definitions execute against
                // the class namespace; the names they bind (loop variables and
                // any attributes they mutate) are harvested just below.
                Stmt::If(_) | Stmt::For(_) | Stmt::While(_) | Stmt::ClassDef(_) => {
                    crate::eval::eval_stmt(state, stmt, tools).await?;
                }
                // Docstrings and `pass` carry no class state; anything else in a
                // class body is not modelled and ignored.
                _ => {}
            }
            // Harvest names this statement bound or changed in the live
            // namespace back into the class dict, so the next statement's seed
            // (and its reads) see the current values — a `for` that builds up
            // `total` must be visible to a following `while`. A name matching
            // the enclosing scope was only read, not bound, so it is skipped;
            // a name already equal in the class dict needs no rewrap.
            let mut changed: Vec<(String, Value)> = Vec::new();
            for (name, value) in &state.variables {
                if class_attrs.get(name) != Some(value)
                    && class_scope_saved.get(name) != Some(value)
                {
                    changed.push((name.clone(), value.clone()));
                }
            }
            for (name, value) in changed {
                let wrapped =
                    wrap_enum_member(enum_kind, class_name, &name, value, &mut enum_auto_next);
                class_attrs.insert(name, wrapped);
            }
        }
        Ok(())
    }
    .await;

    // Restore the enclosing scope: class-body names live in the class dict,
    // not the surrounding namespace.
    state.variables = class_scope_saved;
    body_result?;

    // C3 linearization gives the method resolution order for this
    // class. Each base's MRO is in the registry already (because B1's
    // single-pass eval registers bases before their derivatives).
    let mro = build_mro(class_name, &bases, &state.classes)?;
    let bases_for_hook = bases.clone();
    // Snapshot class attrs for PEP 487 `__set_name__` before move into registry.
    let attrs_for_set_name: Vec<(String, Value)> =
        class_attrs.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

    // Class-body `__slots__ = ('x', 'y')` / `"x"` / `["x"]`.
    // Inherit parent slot names when any base uses slots (CPython merges).
    let (mut slots, mut slot_names) = parse_slots_attr(class_attrs.get("__slots__"));
    for base in &bases {
        if let Some(b) = state.classes.get(base) {
            if b.slots {
                slots = true;
                for n in &b.slot_names {
                    if !slot_names.iter().any(|s| s == n) {
                        slot_names.push(n.clone());
                    }
                }
            }
        }
    }

    // CPython's `__abstractmethods__`: names still abstract after this class.
    // Start from this body's `@abstractmethod`s plus any inherited-but-
    // unresolved ones, then drop every name given a concrete definition here.
    let abstract_methods: Vec<String> = {
        let mut unresolved: std::collections::BTreeSet<String> =
            abstract_here.iter().cloned().collect();
        for base in &bases {
            if let Some(bc) = state.classes.get(base) {
                unresolved.extend(bc.abstract_methods.iter().cloned());
            }
        }
        // A concrete (non-abstract) definition in this body resolves the name.
        unresolved.retain(|n| {
            abstract_here.iter().any(|a| a == n)
                || !(methods.contains_key(n)
                    || properties.contains_key(n)
                    || static_methods.contains_key(n)
                    || class_methods.contains_key(n)
                    || class_attrs.contains_key(n))
        });
        unresolved.into_iter().collect()
    };

    state.classes.insert(class_name.to_string(), {
        let mut cv = ClassValue::new(class_name);
        cv.methods = methods;
        cv.class_attrs = class_attrs;
        cv.bases = bases;
        cv.mro = mro;
        cv.properties = properties;
        cv.static_methods = static_methods;
        cv.class_methods = class_methods;
        cv.enum_kind = enum_kind;
        cv.enum_members = enum_members;
        cv.annotations = annotations;
        cv.slots = slots;
        cv.slot_names = slot_names;
        cv.abstract_methods = abstract_methods;
        cv
    });
    state
        .set_variable(class_name, Value::Class(class_name.to_string()))
        .map_err(EvalError::Interpreter)?;

    // `class P(NamedTuple)` — turn the annotated class into a namedtuple,
    // reusing the collections.namedtuple builder and layering on field
    // defaults and the user-defined methods from the class body.
    if is_typing_namedtuple {
        finalize_typing_namedtuple(state, class_name)?;
    }

    // Metaclass `__new__` / `__init__` (PEP 3115 subset).
    if let Some(meta) = metaclass_name {
        // Preserve method tables across type() rebuilds that only carry attrs.
        let saved_methods = state.classes.get(class_name).map(|c| {
            (
                c.methods.clone(),
                c.properties.clone(),
                c.static_methods.clone(),
                c.class_methods.clone(),
            )
        });
        invoke_metaclass_new(state, class_name, &meta, tools).await?;
        if let Some((methods, properties, static_methods, class_methods)) = saved_methods {
            if let Some(cv) = state.classes.get_mut(class_name) {
                if cv.methods.is_empty() && !methods.is_empty() {
                    cv.methods = methods;
                    cv.properties = properties;
                    cv.static_methods = static_methods;
                    cv.class_methods = class_methods;
                }
            }
        }
        invoke_metaclass_init(state, class_name, &meta, tools).await?;
    }

    // PEP 487: after the class dict is built, call `__set_name__(cls, name)`
    // on each attribute that defines it (descriptors / named objects).
    invoke_set_name(state, class_name, &attrs_for_set_name, tools).await?;

    // PEP 487: invoke each base's `__init_subclass__` (classmethod-style)
    // with the newly created class. Walk direct bases only; each base
    // is responsible for chaining to its own parents if needed.
    invoke_init_subclass(state, class_name, &bases_for_hook, &init_subclass_kwargs, tools).await?;

    // Apply class-level decorators in REVERSE order (bottom-up,
    // matching CPython): the innermost decorator wraps the class
    // first. Class decorators take the class and return a value
    // — usually the same class transformed, but anything callable is
    // accepted. The result is rebound to the class name in scope.
    if !node.decorator_list.is_empty() {
        let mut result = Value::Class(class_name.to_string());
        for decorator in node.decorator_list.iter().rev() {
            let dec_val = eval_expr(state, decorator, tools).await?;
            result = apply_decorator(state, &dec_val, result, tools).await?;
        }
        state.set_variable(class_name, result).map_err(EvalError::Interpreter)?;
    }

    Ok(Value::None)
}

/// Whether a decorator is `@abstractmethod` — as a bare name, `abc.`-qualified,
/// or one of the deprecated `abstractproperty`/`abstractclassmethod`/
/// `abstractstaticmethod` variants.
fn is_abstractmethod_decorator(decorator: &Expr) -> bool {
    let name = match decorator {
        Expr::Name(n) => n.id.as_str(),
        Expr::Attribute(a) => a.attr.as_str(),
        _ => return false,
    };
    crate::eval::modules::abc_mod::ABSTRACT_DECORATORS.contains(&name)
}

/// Dispatch a method's decorator list to the right ClassValue bucket
/// and insert its body into `state.function_bodies` under a key that
/// stays unique across the three property accessors. Regular methods
/// use the plain `Class.name` key; property accessors disambiguate
/// with a `__get` / `__set` / `__del` suffix so a `@balance.setter`
/// doesn't overwrite the getter's body.
#[expect(
    clippy::too_many_arguments,
    reason = "the buckets travel as four separate `&mut BTreeMap`s by design; bundling them into a struct would just reify the same fan-out at the call site"
)]
fn classify_decorated_method(
    state: &mut InterpreterState,
    decorators: &[Expr],
    class_name: &str,
    method_name: String,
    params: crate::value::FunctionParams,
    body: Vec<Stmt>,
    source: String,
    methods: &mut BTreeMap<String, FunctionDef>,
    properties: &mut BTreeMap<String, PropertyDef>,
    static_methods: &mut BTreeMap<String, FunctionDef>,
    class_methods: &mut BTreeMap<String, FunctionDef>,
) -> Result<(), EvalError> {
    let mut register = |key: String, body: Vec<Stmt>, source: String| -> FunctionDef {
        // Walk the method body for `assigned_names` / `global_names`
        // so `VariableCheckpoint` can snapshot only the names this
        // frame can touch instead of cloning all of `state.variables`
        // per call. `nonlocal` isn't supported inside class methods
        // today (it would route through a cell at the enclosing
        // function scope, not the class), so nonlocal_names stays
        // empty.
        let (mut assigned_names, global_names) =
            crate::eval::functions::collect_assigned_names(&body);
        assigned_names.retain(|n| !global_names.contains(n));
        let is_generator = crate::eval::functions::contains_yield_stmts(&body);
        let docstring = crate::eval::functions::extract_docstring(&body);
        state.function_bodies.insert(key.clone(), Arc::new(body));
        FunctionDef {
            name: key,
            body_key: String::new(),
            wraps_name: None,
            params: params.clone(),
            closure: BTreeMap::new(),
            source,
            nonlocal_names: Vec::new(),
            is_generator,
            nonlocal_cell_id: None,
            assigned_names,
            global_names,
            // Methods have an empty closure, so overlay suppression
            // is moot here. The flag is kept consistent with how
            // top-level `def` would have been classified.
            is_module_level: false,
            docstring,
            cell_refreshes: Vec::new(),
        }
    };
    // No decorators: regular method.
    if decorators.is_empty() {
        let key = format!("{class_name}.{method_name}");
        let func = register(key, body, source);
        methods.insert(method_name, func);
        return Ok(());
    }
    // Single decorator is the common case for the four builtin shapes.
    if decorators.len() == 1 {
        match &decorators[0] {
            Expr::Name(n) if n.id.as_str() == "property" => {
                let key = format!("{class_name}.{method_name}__get");
                let func = register(key, body, source);
                properties.insert(
                    method_name,
                    PropertyDef { getter: func, setter: None, deleter: None, cached: false },
                );
                return Ok(());
            }
            // `@cached_property` (functools) or `@functools.cached_property`: a
            // getter that memoises into the instance dict on first access.
            Expr::Name(n) if n.id.as_str() == "cached_property" => {
                let key = format!("{class_name}.{method_name}__get");
                let func = register(key, body, source);
                properties.insert(
                    method_name,
                    PropertyDef { getter: func, setter: None, deleter: None, cached: true },
                );
                return Ok(());
            }
            Expr::Attribute(a) if a.attr.as_str() == "cached_property" => {
                let key = format!("{class_name}.{method_name}__get");
                let func = register(key, body, source);
                properties.insert(
                    method_name,
                    PropertyDef { getter: func, setter: None, deleter: None, cached: true },
                );
                return Ok(());
            }
            Expr::Name(n) if n.id.as_str() == "staticmethod" => {
                let key = format!("{class_name}.{method_name}__static");
                let func = register(key, body, source);
                static_methods.insert(method_name, func);
                return Ok(());
            }
            Expr::Name(n) if n.id.as_str() == "classmethod" => {
                let key = format!("{class_name}.{method_name}__class");
                let func = register(key, body, source);
                class_methods.insert(method_name, func);
                return Ok(());
            }
            // `@x.setter` / `@x.deleter`: attribute access on an
            // existing property. We resolve x by name in the current
            // class scope (the method_name must equal x.attr — Python
            // requires the property and accessor share the same name).
            Expr::Attribute(attr) => {
                if let Expr::Name(prop_name) = attr.value.as_ref() {
                    let prop_key = prop_name.id.as_str().to_string();
                    let kind = attr.attr.as_str();
                    if properties.contains_key(&prop_key) {
                        let suffix = match kind {
                            "setter" => "__set",
                            "deleter" => "__del",
                            _ => "",
                        };
                        if !suffix.is_empty() {
                            let key = format!("{class_name}.{prop_key}{suffix}");
                            let func = register(key, body, source);
                            if let Some(prop) = properties.get_mut(&prop_key) {
                                match kind {
                                    "setter" => {
                                        prop.setter = Some(func);
                                        return Ok(());
                                    }
                                    "deleter" => {
                                        prop.deleter = Some(func);
                                        return Ok(());
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Err(InterpreterError::Security(format!(
        "method-level decorator stack on '{method_name}' is not one of the supported \
         shapes (@property, @<name>.setter, @<name>.deleter, @staticmethod, @classmethod). \
         See CONFORMANCE.md#unsupported-language-features.",
    ))
    .into())
}

/// Apply a single decorator (any callable Value) to a target Value.
/// Used by class-decorator and function-decorator paths. Only
/// Function / Lambda values are accepted as callables; other forms
/// (Class instances with __call__, builtin sentinels) are out of scope
/// for B2 and rejected with a clear error.
pub(crate) async fn apply_decorator(
    state: &mut InterpreterState,
    decorator: &Value,
    target: Value,
    tools: &Tools,
) -> EvalResult {
    let kwargs: IndexMap<String, Value> = IndexMap::new();
    match decorator {
        Value::Function(def) => {
            let positional = [target];
            call_user_function(state, def, &positional, &kwargs, tools).await
        }
        Value::Lambda(def) => {
            let positional = [target];
            call_lambda(state, def, &positional, &kwargs, tools).await
        }
        // `@dataclasses.dataclass` (bare form): rewrites the target
        // class in-place to record its [`DataclassField`] list. The
        // class identity does not change — the same `Value::Class`
        // handle is returned. The same path handles
        // `dataclasses.dataclass(C)` (call form), which is
        // `@dataclass`-equivalent per CPython.
        Value::ModuleFunction { module, name }
            if module == "dataclasses" && name == "dataclass" =>
        {
            let class_name = match &target {
                Value::Class(n) => n.clone(),
                other => {
                    return Err(InterpreterError::TypeError(format!(
                        "@dataclass requires a class target (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            crate::eval::modules::dataclasses::apply_dataclass(state, &class_name, &kwargs)?;
            Ok(Value::Class(class_name))
        }
        // @lru_cache / @cache as bare ModuleFunction decorator.
        Value::ModuleFunction { module, name }
            if module == "functools" && (name == "lru_cache" || name == "cache") =>
        {
            let maxsize = if name == "cache" { None } else { Some(128) };
            Ok(crate::eval::modules::functools::make_lru_cache_pub(target, maxsize))
        }
        // General bare ModuleFunction decorator (`@contextmanager`,
        // `@functools.wraps`-style, …): call `module.name(target)`.
        Value::ModuleFunction { module, name } => {
            crate::eval::modules::call_function(
                state,
                module,
                name,
                std::slice::from_ref(&target),
                &kwargs,
                tools,
            )
            .await
        }
        // Partial: `@dataclass(frozen=True)` carries kwargs; `@lru_cache(n)`
        // carries maxsize. Generic path: call the partial with the target.
        Value::Partial(data) => {
            if let Value::ModuleFunction { module, name } = &data.func {
                if module == "dataclasses" && name == "dataclass" {
                    let class_name = match &target {
                        Value::Class(n) => n.clone(),
                        other => {
                            return Err(InterpreterError::TypeError(format!(
                                "@dataclass requires a class target (got '{}')",
                                other.type_name()
                            ))
                            .into());
                        }
                    };
                    crate::eval::modules::dataclasses::apply_dataclass(
                        state,
                        &class_name,
                        &data.keywords,
                    )?;
                    return Ok(Value::Class(class_name));
                }
            }
            // General: decorator = partial; result = partial(target)
            let mut combined = data.args.clone();
            combined.push(target);
            crate::eval::functions::call_value_as_function(
                state,
                &data.func,
                &combined,
                &indexmap::IndexMap::new(),
                tools,
            )
            .await
        }
        other => Err(InterpreterError::TypeError(format!(
            "decorator is not callable (got '{}')",
            other.type_name()
        ))
        .into()),
    }
}

/// Wrap a class-body assignment as an EnumMember when the class is
/// an enum subclass. Methods (FunctionDef / Lambda) inside an enum
/// body stay unwrapped so they can be invoked. Non-method values
/// (the typical case: `RED = 1`) become EnumMember.
/// Call `attr.__set_name__(owner, name)` for each class attribute that
/// defines `__set_name__` (PEP 487).
async fn invoke_set_name(
    state: &mut InterpreterState,
    class_name: &str,
    attrs: &[(String, Value)],
    tools: &Tools,
) -> Result<(), EvalError> {
    let owner = Value::Class(class_name.to_string());
    let empty_kwargs = IndexMap::new();
    for (name, value) in attrs {
        let Value::Instance(inst) = value else {
            continue;
        };
        let Some((_, def)) = lookup_method_in_mro(state, &inst.class_name, "__set_name__") else {
            continue;
        };
        let name_val = Value::String(name.as_str().into());
        let call = CallArgs { positional: &[owner.clone(), name_val], keyword: &empty_kwargs };
        let (_ret, _self) = call_method(state, &def, value.clone(), call, tools).await?;
    }
    Ok(())
}

/// Call `Base.__init_subclass__(cls)` for each direct base that defines it.
/// CPython always treats `__init_subclass__` as a classmethod; the new
/// subclass is passed as the first argument (`cls`).
/// Turn an annotated `class P(NamedTuple)` into a namedtuple: build the field
/// machinery via the collections.namedtuple builder, then restore the field
/// defaults and the user-defined methods that were on the class body.
fn finalize_typing_namedtuple(
    state: &mut InterpreterState,
    class_name: &str,
) -> Result<(), EvalError> {
    let (field_vals, default_values, user_methods) = {
        let class = state
            .classes
            .get(class_name)
            .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(class_name)))?;
        let fields = class.annotations.clone();
        // Defaults come from the class attribute matching each field name; they
        // must be trailing (CPython requires it), so collecting only the ones
        // present yields the right suffix of default values.
        let default_values: Vec<Value> =
            fields.iter().filter_map(|f| class.class_attrs.get(f).cloned()).collect();
        let field_vals: Vec<Value> =
            fields.iter().map(|f| Value::String(f.as_str().into())).collect();
        (field_vals, default_values, class.methods.clone())
    };
    // Build the namedtuple machinery (this replaces the class registration).
    crate::eval::modules::collections::call_namedtuple_with_state(
        state,
        &[Value::String(class_name.into()), Value::Tuple(field_vals)],
    )?;
    // Restore the user's own methods and apply the field defaults to __init__.
    if let Some(class) = state.classes.get_mut(class_name) {
        for (name, def) in user_methods {
            class.methods.entry(name).or_insert(def);
        }
        if let Some(init) = class.methods.get_mut("__init__") {
            // bind_params counts trailing defaults from `defaults` (source
            // strings); `default_values` supplies the actual values and wins,
            // so the placeholders are never re-parsed.
            init.params.defaults = vec!["None".to_string(); default_values.len()];
            init.params.default_values = default_values;
        }
    }
    Ok(())
}

async fn invoke_init_subclass(
    state: &mut InterpreterState,
    class_name: &str,
    bases: &[String],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> Result<(), EvalError> {
    let new_cls = Value::Class(class_name.to_string());
    for base in bases {
        // Look on the base's own MRO for __init_subclass__ (classmethod or plain).
        let method = lookup_class_method(state, base, "__init_subclass__")
            .or_else(|| lookup_method_in_mro(state, base, "__init_subclass__").map(|(_, d)| d));
        let Some(def) = method else {
            continue;
        };
        // CPython invokes exactly one `__init_subclass__` (the nearest in the
        // new class's MRO) with the class keyword arguments; its own
        // `super().__init_subclass__(**kwargs)` chains to the rest. Invoke the
        // first defining base and stop so those kwargs aren't consumed twice.
        let call = CallArgs { positional: &[], keyword: kwargs };
        let (_ret, _self) = call_method(state, &def, new_cls.clone(), call, tools).await?;
        break;
    }
    Ok(())
}

/// Bind one class-body assignment target to `value`, inserting into
/// `class_attrs`. Handles a plain `Name` and tuple/list unpacking
/// (`X, Y = 1, 2`), recursing for nested targets.
fn bind_class_target(
    target: &Expr,
    value: &Value,
    enum_kind: Option<crate::value::EnumKind>,
    class_name: &str,
    class_attrs: &mut BTreeMap<String, Value>,
    auto_next: &mut i64,
) -> Result<(), EvalError> {
    match target {
        Expr::Name(name) => {
            let attr = name.id.as_str().to_string();
            let wrapped = wrap_enum_member(enum_kind, class_name, &attr, value.clone(), auto_next);
            class_attrs.insert(attr, wrapped);
            Ok(())
        }
        Expr::Tuple(t) => {
            unpack_class_targets(&t.elts, value, enum_kind, class_name, class_attrs, auto_next)
        }
        Expr::List(l) => {
            unpack_class_targets(&l.elts, value, enum_kind, class_name, class_attrs, auto_next)
        }
        // Attribute / subscript / starred class-body targets are unusual and
        // left unmodelled (as before).
        _ => Ok(()),
    }
}

/// Unpack an iterable `value` across `elts` class-body targets, requiring the
/// arity to match (CPython raises ValueError otherwise).
fn unpack_class_targets(
    elts: &[Expr],
    value: &Value,
    enum_kind: Option<crate::value::EnumKind>,
    class_name: &str,
    class_attrs: &mut BTreeMap<String, Value>,
    auto_next: &mut i64,
) -> Result<(), EvalError> {
    let items: Vec<Value> = match value {
        Value::Tuple(items) => items.clone(),
        Value::List(items) => items.lock().clone(),
        Value::String(s) => s.chars().map(|c| Value::String(c.to_string().into())).collect(),
        _ => {
            return Err(InterpreterError::TypeError(format!(
                "cannot unpack non-iterable {} object",
                value.type_name()
            ))
            .into());
        }
    };
    if items.len() != elts.len() {
        return Err(InterpreterError::ValueError(if items.len() > elts.len() {
            format!("too many values to unpack (expected {})", elts.len())
        } else {
            format!("not enough values to unpack (expected {}, got {})", elts.len(), items.len())
        })
        .into());
    }
    for (elt, item) in elts.iter().zip(items) {
        bind_class_target(elt, &item, enum_kind, class_name, class_attrs, auto_next)?;
    }
    Ok(())
}

fn wrap_enum_member(
    enum_kind: Option<crate::value::EnumKind>,
    class_name: &str,
    member_name: &str,
    value: Value,
    auto_next: &mut i64,
) -> Value {
    let Some(kind) = enum_kind else { return value };
    // Don't wrap methods / lambdas — they're enum methods.
    if matches!(value, Value::Function(_) | Value::Lambda(_)) {
        return value;
    }
    // Don't double-wrap. Don't wrap dunders (_value_ etc.) — they're
    // metadata.
    if matches!(value, Value::EnumMember { .. }) || member_name.starts_with('_') {
        return value;
    }
    // Resolve `auto()`: StrEnum yields the lowercased member name; a Flag/IntFlag
    // yields the next unused power of two (bit position); any other enum yields
    // the next sequential integer. An explicit int advances the auto counter so
    // a following `auto()` continues from there (CPython semantics).
    let value = if crate::eval::modules::enum_mod::is_auto_sentinel(&value) {
        if matches!(kind, crate::value::EnumKind::Str) {
            Value::String(member_name.to_lowercase().into())
        } else if kind.is_flag() {
            // The next power of two (bit position); auto_next starts at 1, so the
            // flag members come out 1, 2, 4, 8, ….
            let n = u64::try_from((*auto_next).max(1)).unwrap_or(1).next_power_of_two();
            *auto_next = i64::try_from(n.saturating_mul(2)).unwrap_or(i64::MAX);
            Value::Int(i64::try_from(n).unwrap_or(i64::MAX))
        } else {
            let n = *auto_next;
            *auto_next = n + 1;
            Value::Int(n)
        }
    } else {
        if let Value::Int(i) = &value {
            *auto_next = (*auto_next).max(i.saturating_add(1));
        }
        value
    };
    Value::EnumMember {
        class_name: class_name.to_string(),
        member_name: member_name.to_string(),
        value: Box::new(value),
        kind,
    }
}

/// Compute the C3-linearized MRO for `class_name` over `bases`. The
/// algorithm is CPython's: the MRO of a class is itself, followed by the
/// merge of (a) the MROs of each base in declaration order and (b) the
/// list of bases themselves. The merge picks the head of the first list
/// whose head does not appear in the tail of any other list; if no such
/// head exists, raise `TypeError` matching CPython's exact wording.
/// Source: <https://en.wikipedia.org/wiki/C3_linearization>.
fn build_mro(
    class_name: &str,
    bases: &[String],
    registry: &rustc_hash::FxHashMap<String, ClassValue>,
) -> Result<Vec<String>, EvalError> {
    let mut sequences: Vec<Vec<String>> = bases
        .iter()
        .map(|b| registry.get(b).map_or_else(|| vec![b.clone()], |cls| cls.mro.clone()))
        .collect();
    sequences.push(bases.to_vec());

    let mut result: Vec<String> = vec![class_name.to_string()];

    while !sequences.iter().all(Vec::is_empty) {
        // Find a "good head" — a class that appears at the head of some
        // non-empty sequence and nowhere in the tail of any other.
        let candidate = sequences
            .iter()
            .filter_map(|seq| seq.first())
            .find(|head| sequences.iter().all(|seq| !seq.iter().skip(1).any(|n| n == *head)))
            .cloned();

        let Some(head) = candidate else {
            // CPython 3.12 includes a literal newline between
            // "resolution" and "order" in the format string. The
            // surrounding character-for-character match is what lets a
            // planner LLM trained on cpython tracebacks recognise the
            // error precisely.
            return Err(InterpreterError::TypeError(format!(
                "Cannot create a consistent method resolution\norder (MRO) for bases of class '{class_name}'"
            ))
            .into());
        };

        result.push(head.clone());

        // Strip `head` from the front of every sequence where it appears.
        for seq in &mut sequences {
            if seq.first() == Some(&head) {
                seq.remove(0);
            }
        }
        sequences.retain(|seq| !seq.is_empty());
    }

    Ok(result)
}

/// Whether a class's MRO reaches `Exception` / `BaseException` (so its
/// instances are exception objects).
fn class_mro_is_exception(class: &ClassValue) -> bool {
    class.mro.iter().any(|b| {
        b == "Exception"
            || b == "BaseException"
            || crate::eval::functions::is_exception_type_name(b)
    })
}

/// CPython's `str(exc)` for a `BaseException` built from `args`:
/// empty for no args, `str(arg)` for a single arg, and the tuple's
/// repr (`(1, 2)`) for multiple.
fn exception_message_from_args(args: &[Value]) -> String {
    match args {
        [] => String::new(),
        [single] => format!("{single}"),
        _ => format!("{}", Value::Tuple(args.to_vec())),
    }
}

/// Convert an exception-subclass instance (after its `__init__` ran)
/// into a `Value::Exception`. The `args` field — set by the
/// object-level `super().__init__(*args)` — drives `e.args` and the
/// `str(e)` message; every other attribute becomes a custom field so
/// `except E as e: e.code` resolves.
fn instance_to_exception(class_name: &str, inst: &InstanceValue) -> crate::value::ExceptionValue {
    let fields = inst.fields.lock();
    let args: Vec<Value> = match fields.get("args") {
        Some(Value::Tuple(items)) => items.clone(),
        Some(Value::List(items)) => items.lock().clone(),
        _ => Vec::new(),
    };
    let mut exc = crate::value::ExceptionValue::new(
        class_name.to_string(),
        exception_message_from_args(&args),
    )
    .with_args(args);
    for (name, value) in fields.iter() {
        if name != "args" {
            exc.fields.insert(name.clone(), value.clone());
        }
    }
    exc
}

/// Instantiate `class_name(args, kwargs)`: allocate an empty instance and run
/// `__init__` if the class defines one.
pub async fn instantiate(
    state: &mut InterpreterState,
    class_name: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> EvalResult {
    // An abstract class (unresolved `@abstractmethod`s) can't be instantiated.
    if let Some(class) = state.classes.get(class_name) {
        if !class.abstract_methods.is_empty() {
            let names = class
                .abstract_methods
                .iter()
                .map(|m| format!("'{m}'"))
                .collect::<Vec<_>>()
                .join(", ");
            let noun = if class.abstract_methods.len() == 1 { "method" } else { "methods" };
            return Err(crate::error::EvalError::Exception(crate::value::ExceptionValue::new(
                "TypeError",
                format!(
                    "Can't instantiate abstract class {class_name} without an implementation for abstract {noun} {names}"
                ),
            )));
        }
    }
    // Enum value-construction: `Color(1)` returns the member whose
    // value equals the arg. Customer pattern is
    // `Color.from_value = Color(value)`; matching CPython's
    // `Enum(value)` constructor.
    if let Some(class) = state.classes.get(class_name) {
        if class.enum_kind.is_some() && args.len() == 1 && kwargs.is_empty() {
            let needle = &args[0];
            for (member_name, member_value) in &class.class_attrs {
                if let Value::EnumMember { value, .. } = member_value {
                    if crate::eval::operations::values_equal_pub(value, needle) {
                        return Ok(member_value.clone());
                    }
                    let _ = member_name;
                }
            }
            return Err(crate::error::EvalError::Exception(crate::value::ExceptionValue::new(
                "ValueError",
                format!("{needle} is not a valid {class_name}"),
            )));
        }
        // Exception subclasses: `class E(Exception): ...; raise E("msg")`
        // constructs a Value::Exception with the class name as type_name so
        // except-matching can walk the MRO. Mirrors ExceptionType constructors.
        let is_exception_subclass = class_mro_is_exception(class);
        if is_exception_subclass {
            // A user-defined `__init__` may set custom attributes
            // (`self.code = ...`) and/or forward a computed message via
            // `super().__init__(...)`. Run it on a real instance and
            // fold the resulting fields into the Exception so
            // `except E as e: e.code` and `str(e)` both work. When no
            // user `__init__` exists, the args ARE the exception args
            // directly (CPython's `BaseException.__init__`).
            let Some((_defining_class, init_def)) =
                lookup_method_in_mro(state, class_name, "__init__")
            else {
                return Ok(Value::Exception(Box::new(
                    crate::value::ExceptionValue::new(
                        class_name.to_string(),
                        exception_message_from_args(args),
                    )
                    .with_args(args.to_vec()),
                )));
            };
            // CPython's `BaseException.__new__(cls, *args)` seeds
            // `self.args` from the constructor args before `__init__`
            // runs, so an `__init__` that never calls `super().__init__`
            // still reports the original args (and `str(self)`). A
            // `super().__init__(*other)` overwrites this seed.
            let mut seed = BTreeMap::new();
            seed.insert("args".to_string(), Value::Tuple(args.to_vec()));
            let instance = Value::Instance(InstanceValue {
                class_name: class_name.to_string(),
                fields: crate::value::shared_fields(seed),
            });
            let call = CallArgs { positional: args, keyword: kwargs };
            let (_returned, configured_self) =
                call_method(state, &init_def, instance, call, tools).await?;
            let Value::Instance(inst) = configured_self else {
                // A user `__init__` cannot legally return a non-instance
                // (CPython raises `TypeError: __init__() should return
                // None`); fall back to the plain arg-based construction.
                return Ok(Value::Exception(Box::new(
                    crate::value::ExceptionValue::new(
                        class_name.to_string(),
                        exception_message_from_args(args),
                    )
                    .with_args(args.to_vec()),
                )));
            };
            return Ok(Value::Exception(Box::new(instance_to_exception(class_name, &inst))));
        }
    }
    // A user-defined `__new__` customizes construction: CPython calls
    // `cls.__new__(cls, *args)`, then `__init__` on the result — but only when
    // the result is an instance of `cls`. Only fires when the class actually
    // defines `__new__`; the default `object.__new__` (a blank instance) is the
    // path below and is what `super().__new__(cls)` inside a user `__new__`
    // resolves to.
    if let Some((_, new_def)) = lookup_method_in_mro(state, class_name, "__new__") {
        let call = CallArgs { positional: args, keyword: kwargs };
        let (obj, _) =
            call_method(state, &new_def, Value::Class(class_name.to_string()), call, tools).await?;
        if let Value::Instance(inst) = &obj {
            let returns_cls = inst.class_name == class_name
                || state
                    .classes
                    .get(&inst.class_name)
                    .is_some_and(|c| c.mro.iter().any(|a| a == class_name));
            if returns_cls {
                if let Some((_, init_def)) = lookup_method_in_mro(state, class_name, "__init__") {
                    let call = CallArgs { positional: args, keyword: kwargs };
                    let (_r, configured) =
                        call_method(state, &init_def, obj.clone(), call, tools).await?;
                    return Ok(configured);
                }
            }
        }
        return Ok(obj);
    }

    let instance = Value::Instance(InstanceValue {
        class_name: class_name.to_string(),
        fields: crate::value::shared_fields(BTreeMap::new()),
    });

    // `@dataclass`-synthesized __init__: if the class is a dataclass
    // and no user-defined `__init__` overrides the synthesis, bind
    // positional + keyword args onto the instance's fields per the
    // declared field order. CPython's @dataclass produces the same
    // mapping; doing it directly avoids generating source code for an
    // `__init__` method just to re-parse and execute it.
    let has_user_init =
        state.classes.get(class_name).is_some_and(|c| c.methods.contains_key("__init__"));
    if !has_user_init {
        if let Some(fields) = state.classes.get(class_name).and_then(|c| c.dataclass_fields.clone())
        {
            return dataclass_instantiate(state, class_name, &fields, args, kwargs, tools).await;
        }
    }

    // Walk the MRO to find an `__init__`. CPython uses
    // `type(instance).__init__`, which after MRO resolution finds the
    // most-derived class that defines it. A class without its own
    // `__init__` inherits its base's.
    let init = lookup_method_in_mro(state, class_name, "__init__");
    let Some((_defining_class, init_def)) = init else {
        // No `__init__` anywhere in the MRO: only a zero-argument call
        // is valid, as in CPython.
        if !args.is_empty() || !kwargs.is_empty() {
            return Err(
                InterpreterError::TypeError(format!("{class_name}() takes no arguments")).into()
            );
        }
        return Ok(instance);
    };
    let call = CallArgs { positional: args, keyword: kwargs };
    let (_returned, configured_self) = call_method(state, &init_def, instance, call, tools).await?;
    Ok(configured_self)
}

/// Bind `args` / `kwargs` onto a dataclass instance's fields per the
/// declared field order, applying defaults / default factories for
/// missing init-fields. Fields whose `init` flag is False are seeded
/// directly from `default` / `default_factory` without participating in
/// the positional/keyword binding.
///
/// Errors match CPython:
///   * `TypeError("'X' got multiple values for argument 'name'")` when a positional and keyword
///     both name the same field.
///   * `TypeError("'X' missing required argument: 'name'")` when an init field has no default and
///     no value was supplied.
///   * `TypeError("'X' got unexpected keyword argument 'name'")` when a keyword arg does not match
///     any init field.
async fn dataclass_instantiate(
    state: &mut InterpreterState,
    class_name: &str,
    fields: &[crate::value::DataclassField],
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> EvalResult {
    let init_fields: Vec<&crate::value::DataclassField> =
        fields.iter().filter(|f| f.init).collect();
    if args.len() > init_fields.len() {
        return Err(InterpreterError::TypeError(format!(
            "{class_name}() takes {} positional arguments but {} were given",
            init_fields.len(),
            args.len()
        ))
        .into());
    }
    // Validate keyword args against known init-field names early so a
    // typo surfaces with the same message CPython produces.
    for key in kwargs.keys() {
        if !init_fields.iter().any(|f| &f.name == key) {
            return Err(InterpreterError::TypeError(format!(
                "{class_name}() got an unexpected keyword argument '{key}'"
            ))
            .into());
        }
    }

    let mut instance_fields: BTreeMap<String, Value> = BTreeMap::new();

    for (index, field) in init_fields.iter().enumerate() {
        let positional = args.get(index).cloned();
        let keyword = kwargs.get(&field.name).cloned();
        let value = match (positional, keyword) {
            (Some(_), Some(_)) => {
                return Err(InterpreterError::TypeError(format!(
                    "{class_name}() got multiple values for argument '{}'",
                    field.name
                ))
                .into());
            }
            (Some(v), None) | (None, Some(v)) => v,
            (None, None) => {
                if let Some(default) = field.default.clone() {
                    default
                } else if let Some(factory) = field.default_factory.clone() {
                    invoke_default_factory(state, &factory, tools).await?
                } else {
                    return Err(InterpreterError::TypeError(format!(
                        "{class_name}() missing required argument: '{}'",
                        field.name
                    ))
                    .into());
                }
            }
        };
        instance_fields.insert(field.name.clone(), value);
    }

    // Non-init fields: seed from default / default_factory unconditionally.
    for field in fields.iter().filter(|f| !f.init) {
        let value = if let Some(default) = field.default.clone() {
            default
        } else if let Some(factory) = field.default_factory.clone() {
            invoke_default_factory(state, &factory, tools).await?
        } else {
            continue;
        };
        instance_fields.insert(field.name.clone(), value);
    }

    Ok(Value::Instance(InstanceValue {
        class_name: class_name.to_string(),
        fields: crate::value::shared_fields(instance_fields),
    }))
}

fn empty_for_builtin_factory(name: &str) -> EvalResult {
    match name {
        "list" => Ok(Value::List(shared_list(Vec::new()))),
        "dict" => Ok(Value::Dict(crate::value::shared_dict(IndexMap::new()))),
        "set" => Ok(Value::new_set(Vec::new())),
        "frozenset" => Ok(Value::new_frozenset(Vec::new())),
        "tuple" => Ok(Value::Tuple(Vec::new())),
        "str" => Ok(Value::String("".into())),
        other => {
            Err(InterpreterError::TypeError(format!("default_factory '{other}' is not callable"))
                .into())
        }
    }
}

async fn invoke_default_factory(
    state: &mut InterpreterState,
    factory: &Value,
    tools: &Tools,
) -> EvalResult {
    let empty_kwargs: IndexMap<String, Value> = IndexMap::new();
    match factory {
        Value::Function(def) => call_user_function(state, def, &[], &empty_kwargs, tools).await,
        Value::Lambda(def) => call_lambda(state, def, &[], &empty_kwargs, tools).await,
        // Boxed: `invoke_default_factory` is called from
        // `dataclass_instantiate`, which is itself called from
        // `instantiate` — without the indirection the async closure
        // type would be infinitely-sized.
        Value::Class(name) => Box::pin(instantiate(state, name, &[], &empty_kwargs, tools)).await,
        Value::ModuleFunction { module, name } => {
            crate::eval::modules::call_function(state, module, name, &[], &empty_kwargs, tools)
                .await
        }
        // Built-in factory names invoked directly (e.g.
        // `default_factory=list`): `Value::Type` is the explicit-type
        // form, `Value::BuiltinName` is what bare-name lookup produces.
        // Both route to the same empty-container shim.
        Value::Type(t) => empty_for_builtin_factory(t.as_str()),
        Value::BuiltinName(name) => empty_for_builtin_factory(name.as_str()),
        other => Err(InterpreterError::TypeError(format!(
            "default_factory is not callable (got '{}')",
            other.type_name()
        ))
        .into()),
    }
}

/// Look up and call `instance.method(args)`, returning `(return_value, self)`
/// where `self` reflects any mutations the method made — the caller writes it
/// back into the receiver's slot.
pub async fn instance_method_call(
    state: &mut InterpreterState,
    instance: Value,
    method_name: &str,
    call: CallArgs<'_>,
    tools: &Tools,
) -> Result<(Value, Value), EvalError> {
    let Value::Instance(inst) = &instance else {
        return Err(
            InterpreterError::Runtime("instance_method_call on a non-instance".into()).into()
        );
    };
    // `contextlib.ExitStack` methods (enter_context/callback/close) run user
    // context managers/callbacks, so they need the async state path here.
    if inst.class_name == crate::eval::modules::contextlib_mod::EXITSTACK_CLASS {
        if let Some(result) = crate::eval::modules::contextlib_mod::try_exitstack_method(
            state,
            &instance,
            method_name,
            call.positional,
            tools,
        )
        .await
        {
            return Ok((result?, instance));
        }
    }
    let class_name = inst.class_name.clone();
    // staticmethod beats regular method per CPython's
    // __getattribute__ order. A staticmethod is called without
    // binding `self` — we route through call_user_function instead of
    // call_method (which expects a receiver).
    if let Some(def) = lookup_static_method(state, &class_name, method_name) {
        let result = call_user_function(state, &def, call.positional, call.keyword, tools).await?;
        return Ok((result, instance));
    }
    // classmethod beats regular method too. The first arg bound is
    // the class (Value::Class), not the instance. The receiver that
    // `call_method` threads back is that class — but the caller writes
    // the returned receiver back over the instance's slot, so return the
    // original `instance` (a classmethod never mutates the instance) to
    // keep `s` an instance after `s.cm()`.
    if let Some(def) = lookup_class_method(state, &class_name, method_name) {
        let (result, _cls) =
            call_method(state, &def, Value::Class(class_name.clone()), call, tools).await?;
        return Ok((result, instance));
    }
    // Walk the MRO: a method defined in any ancestor is callable on
    // `instance`, with `__class__` bound to the defining class so
    // zero-arg `super()` inside that method resumes at the next slot.
    let method = lookup_method_in_mro(state, &class_name, method_name);
    let Some((_defining_class, def)) = method else {
        // Box the fallback future so it does not inflate this hot function's
        // state machine on the recursion path (the `deep_recursion` canary
        // SIGABRTs otherwise — an embedded future here costs native stack).
        return Box::pin(instance_attr_call_fallback(
            state,
            instance,
            &class_name,
            method_name,
            call,
            tools,
        ))
        .await;
    };
    call_method(state, &def, instance, call, tools).await
}

/// `instance.name(...)` when `name` is not a method on the MRO. CPython treats
/// this as `(instance.name)(...)` — an attribute lookup followed by a call — so
/// resolve the attribute the same way plain access does (a callable stored in
/// the instance's own dict, then `__getattr__`) and call the result. Returns
/// the same AttributeError as before when nothing resolves.
async fn instance_attr_call_fallback(
    state: &mut InterpreterState,
    instance: Value,
    class_name: &str,
    method_name: &str,
    call: CallArgs<'_>,
    tools: &Tools,
) -> Result<(Value, Value), EvalError> {
    // A callable stored directly on the instance (`self.cb = fn; self.cb()`).
    let field = match &instance {
        Value::Instance(inst) => inst.fields.lock().get(method_name).cloned(),
        _ => None,
    };
    if let Some(f) = field {
        let result = crate::eval::functions::call_value_as_function(
            state,
            &f,
            call.positional,
            call.keyword,
            tools,
        )
        .await?;
        return Ok((result, instance));
    }
    // `__getattr__` fires on a miss: resolve the attribute, then call it.
    if let Some((_, getattr)) = lookup_method_in_mro(state, class_name, "__getattr__") {
        let attr_arg = Value::String(method_name.into());
        let empty_kwargs = indexmap::IndexMap::new();
        let getattr_call =
            CallArgs { positional: std::slice::from_ref(&attr_arg), keyword: &empty_kwargs };
        let (attr_value, _self) =
            call_method(state, &getattr, instance.clone(), getattr_call, tools).await?;
        let result = crate::eval::functions::call_value_as_function(
            state,
            &attr_value,
            call.positional,
            call.keyword,
            tools,
        )
        .await?;
        return Ok((result, instance));
    }
    Err(InterpreterError::AttributeError(format!(
        "'{class_name}' object has no attribute '{method_name}'"
    ))
    .into())
}

/// Run a method body with `self` bound to `self_value`, returning the method's
/// return value and the post-execution `self` (carrying any mutations).
///
/// Unlike a free function, a method sees the *current* global scope (not a
/// def-time closure snapshot), so no closure is applied — `self` and the
/// parameters are layered over the live variables and removed on return.
/// Apply a method frame's local-scope bindings (parameters including
/// `self`) onto `state.variables`. Extracted as a sync helper so the
/// per-step state doesn't survive across `execute_body(...).await` —
/// see the matching helpers in `eval::functions::mod` for the
/// stack-budget reasoning.
fn apply_method_scope(
    state: &mut InterpreterState,
    local_scope: &rustc_hash::FxHashMap<String, Value>,
) -> Result<(), EvalError> {
    for (name, value) in local_scope {
        state.set_variable(name, value.clone()).map_err(EvalError::Interpreter)?;
    }
    Ok(())
}

pub async fn call_method(
    state: &mut InterpreterState,
    method: &FunctionDef,
    self_value: Value,
    call: CallArgs<'_>,
    tools: &Tools,
) -> Result<(Value, Value), EvalError> {
    // Grow the host stack on demand so deep method recursion doesn't
    // overflow it (see `dispatch::grow_stack`).
    crate::eval::functions::dispatch::grow_stack(call_method_inner(
        state, method, self_value, call, tools,
    ))
    .await
}

async fn call_method_inner(
    state: &mut InterpreterState,
    method: &FunctionDef,
    self_value: Value,
    call: CallArgs<'_>,
    tools: &Tools,
) -> Result<(Value, Value), EvalError> {
    state.enter_call().map_err(EvalError::Interpreter)?;
    // Method frames also own a cell-owners scope; nested `def` inside
    // a method body that declares `nonlocal` registers here.
    state.frame_cell_owners.push(rustc_hash::FxHashMap::default());

    // Push a method frame so zero-arg `super()` can read the defining
    // class + `self` from the top of the stack. The defining class is
    // encoded in the method's qualified name (`Class.method`).
    let defining_class =
        method.name.split_once('.').map_or_else(|| method.name.clone(), |(cls, _)| cls.to_string());
    let self_local_name = method.params.args.first().map(|p| p.name.clone());
    // Push a frame for an instance OR class receiver so zero-arg `super()`
    // works inside both instance methods and classmethods (`__init_subclass__`).
    let frame_pushed = if matches!(&self_value, Value::Instance(_) | Value::Class(_)) {
        state.method_frame_stack.push(crate::state::MethodFrame {
            defining_class,
            self_value: self_value.clone(),
            self_local_name: self_local_name.clone(),
        });
        true
    } else {
        false
    };

    // The receiver is the first positional parameter (conventionally `self`).
    let mut full_args = Vec::with_capacity(call.positional.len() + 1);
    full_args.push(self_value);
    full_args.extend_from_slice(call.positional);

    let local_scope =
        match bind_params(&method.params, &full_args, call.keyword, state, tools).await {
            Ok(scope) => scope,
            Err(e) => {
                if frame_pushed {
                    state.method_frame_stack.pop();
                }
                state.frame_cell_owners.pop();
                state.exit_call();
                return Err(e);
            }
        };
    let self_param = self_local_name.clone();

    // Snapshot only the names this method frame can touch — its
    // parameters (including `self`) plus the statically-collected
    // `assigned_names` from the body walker. Methods don't capture a
    // closure (they look up free names against the live module
    // scope), so the closure bucket is empty here. Same rationale as
    // call_user_function: this replaces an unconditional
    // state.variables.clone() that previously dominated per-call cost.
    let touched: Vec<String> = method
        .params
        .args
        .iter()
        .map(|p| p.name.clone())
        .chain(method.params.vararg.iter().cloned())
        .chain(method.params.kwonlyargs.iter().map(|p| p.name.clone()))
        .chain(method.params.kwarg.iter().cloned())
        .chain(method.assigned_names.iter().cloned())
        .filter(|n| !method.global_names.contains(n))
        .collect();
    let checkpoint = crate::eval::functions::VariableCheckpoint::capture(state, &touched);

    // Apply param/self bindings via a sync helper — same future-size
    // reasoning as `call_user_function`.
    if let Err(e) = apply_method_scope(state, &local_scope) {
        checkpoint.restore(state);
        if frame_pushed {
            state.method_frame_stack.pop();
        }
        state.frame_cell_owners.pop();
        state.exit_call();
        return Err(e);
    }

    let body = state.function_bodies.get(&method.name).cloned();
    let exec_result = match body {
        // A generator method (its body uses `yield`) returns a generator rather
        // than running to completion — `def __iter__(self): ... yield ...` and
        // any other yielding method. Collected eagerly into a Lazy generator
        // (same shape as the while-based generator fallback).
        Some(stmts) if method.is_generator => {
            state.yield_stack.push(Vec::new());
            let body_result = execute_body(state, stmts.as_slice(), tools).await;
            let collected = state.yield_stack.pop().unwrap_or_default();
            match body_result {
                Ok(_) | Err(EvalError::Signal(crate::error::ControlFlow::Return(_))) => {
                    let cursor_id = state.next_cursor_id;
                    state.next_cursor_id = state.next_cursor_id.wrapping_add(1);
                    state.lazy_cursors.insert(cursor_id, 0);
                    Ok(Value::Lazy { items: collected, cursor_id })
                }
                Err(e) => Err(e),
            }
        }
        Some(stmts) => execute_body(state, stmts.as_slice(), tools).await,
        None => Ok(Value::None),
    };

    // Capture the possibly-mutated `self` before the frame's scope is dropped.
    let configured_self =
        self_param.and_then(|name| state.variables.get(&name).cloned()).unwrap_or(Value::None);

    checkpoint.restore(state);
    if frame_pushed {
        state.method_frame_stack.pop();
    }
    state.frame_cell_owners.pop();
    state.exit_call();

    let returned = match exec_result {
        Ok(val) => val,
        Err(EvalError::Signal(crate::error::ControlFlow::Return(val))) => *val,
        Err(e) => return Err(e),
    };
    Ok((returned, configured_self))
}

/// Read `instance.attr`: instance field, then walk the class's MRO
/// for a class attribute. Property dispatch lives one level up in
/// `eval/names.rs::eval_attribute` because invoking a getter requires
/// async; this sync helper handles the non-descriptor cases.
pub fn instance_attribute(
    state: &InterpreterState,
    inst: &InstanceValue,
    attr: &str,
) -> EvalResult {
    if let Some(value) = inst.fields.lock().get(attr) {
        return Ok(value.clone());
    }
    if let Some(value) = lookup_class_attr(state, &inst.class_name, attr) {
        return Ok(value);
    }
    Err(InterpreterError::AttributeError(format!(
        "'{}' object has no attribute '{attr}'",
        inst.class_name
    ))
    .into())
}

/// Walk MRO for a class attribute that is a user instance (descriptor
/// candidate). First hit wins.
pub fn lookup_class_attr_instance(
    state: &InterpreterState,
    class_name: &str,
    attr: &str,
) -> Option<InstanceValue> {
    let class = state.classes.get(class_name)?;
    for ancestor_name in &class.mro {
        if let Some(ancestor) = state.classes.get(ancestor_name) {
            if let Some(Value::Instance(inst)) = ancestor.class_attrs.get(attr) {
                return Some(inst.clone());
            }
        }
    }
    None
}

/// Walk `class_name`'s MRO looking for a `@property` descriptor named
/// `attr`. Returns the first match.
pub fn lookup_property(
    state: &InterpreterState,
    class_name: &str,
    attr: &str,
) -> Option<PropertyDef> {
    let class = state.classes.get(class_name)?;
    for ancestor_name in &class.mro {
        if let Some(ancestor) = state.classes.get(ancestor_name) {
            if let Some(prop) = ancestor.properties.get(attr) {
                return Some(prop.clone());
            }
        }
    }
    None
}

/// Walk `class_name`'s MRO looking for a `@staticmethod`-marked entry.
pub fn lookup_static_method(
    state: &InterpreterState,
    class_name: &str,
    method_name: &str,
) -> Option<FunctionDef> {
    let class = state.classes.get(class_name)?;
    for ancestor_name in &class.mro {
        if let Some(ancestor) = state.classes.get(ancestor_name) {
            if let Some(def) = ancestor.static_methods.get(method_name) {
                return Some(def.clone());
            }
        }
    }
    None
}

/// Walk `class_name`'s MRO looking for a `@classmethod`-marked entry.
pub fn lookup_class_method(
    state: &InterpreterState,
    class_name: &str,
    method_name: &str,
) -> Option<FunctionDef> {
    let class = state.classes.get(class_name)?;
    for ancestor_name in &class.mro {
        if let Some(ancestor) = state.classes.get(ancestor_name) {
            if let Some(def) = ancestor.class_methods.get(method_name) {
                return Some(def.clone());
            }
        }
    }
    None
}

/// Invoke a `@property` getter: call_method binds `instance` as `self`
/// and the property's body runs. The getter takes no extra arguments.
/// Post-call self-mutation is captured by call_method but discarded
/// here — getters that mutate `self` are a contortion CPython doesn't
/// guarantee to support uniformly.
pub async fn invoke_property_getter(
    state: &mut InterpreterState,
    getter: &FunctionDef,
    instance: Value,
    cache_key: Option<&str>,
    tools: &Tools,
) -> EvalResult {
    // `functools.cached_property` (cache_key is Some): the instance dict shadows
    // the descriptor, so return a stored value directly and otherwise run the
    // getter once and store the result into the shared fields (visible on every
    // alias). A plain `@property` (cache_key None) always re-runs the getter.
    if let Some(key) = cache_key {
        if let Value::Instance(inst) = &instance {
            if let Some(v) = inst.fields.lock().get(key) {
                return Ok(v.clone());
            }
        }
    }
    let call = CallArgs { positional: &[], keyword: &IndexMap::new() };
    let (returned, _self) = call_method(state, getter, instance.clone(), call, tools).await?;
    if let Some(key) = cache_key {
        if let Value::Instance(inst) = &instance {
            inst.fields.lock().insert(key.to_string(), returned.clone());
        }
    }
    Ok(returned)
}

/// Invoke a `@property` setter: call_method binds `instance` as
/// `self` and the value as the explicit setter arg. The configured
/// self is returned so the caller can write it back to the receiver
/// slot.
pub async fn invoke_property_setter(
    state: &mut InterpreterState,
    setter: &FunctionDef,
    instance: Value,
    value: Value,
    tools: &Tools,
) -> Result<Value, EvalError> {
    let call = CallArgs { positional: &[value], keyword: &IndexMap::new() };
    let (_returned, configured_self) = call_method(state, setter, instance, call, tools).await?;
    Ok(configured_self)
}

/// Invoke a `@property` deleter: call_method binds `instance` as
/// `self`. The configured self is returned for write-back.
pub async fn invoke_property_deleter(
    state: &mut InterpreterState,
    deleter: &FunctionDef,
    instance: Value,
    tools: &Tools,
) -> Result<Value, EvalError> {
    let call = CallArgs { positional: &[], keyword: &IndexMap::new() };
    let (_returned, configured_self) = call_method(state, deleter, instance, call, tools).await?;
    Ok(configured_self)
}

/// Read `Class.attr`: `__name__`/`__qualname__`, then walk the MRO
/// for staticmethods, classmethods, and class attributes. Returns the
/// callable directly for static/class methods so `Math.add(...)` and
/// `Counter.factory(...)` both work without going through the
/// instance-method path.
pub fn class_attribute(state: &InterpreterState, class_name: &str, attr: &str) -> EvalResult {
    if attr == "__name__" || attr == "__qualname__" {
        return Ok(Value::String(class_name.into()));
    }
    if let Some(def) = lookup_static_method(state, class_name, attr) {
        return Ok(Value::Function(std::sync::Arc::new(def)));
    }
    if let Some(def) = lookup_class_method(state, class_name, attr) {
        // Bind the class as the first arg by wrapping in a class-method
        // marker. For now, expose the bare FunctionDef and rely on the
        // call path to prepend Value::Class(class_name) — the
        // dispatch fast-path for `Class.method(...)` calls `call_method`
        // with the class as receiver, which already binds it as the
        // first param.
        let _ = def;
        // Returning a method-marker sentinel so the call path handles
        // binding. The sentinel re-uses the legacy `__method__self__`
        // shape; the call evaluator's class-method dispatch builds the
        // bound call.
        return Ok(Value::UnboundClassMethod {
            class: class_name.to_string(),
            method: attr.to_string(),
        });
    }
    if let Some(value) = lookup_class_attr(state, class_name, attr) {
        return Ok(value);
    }
    // A regular method accessed through the class (`C.method`, not `instance.method`)
    // is the plain function in CPython — callable as `C.method(instance, ...)` with
    // the receiver passed explicitly. Return the underlying FunctionDef.
    if let Some((_, def)) = lookup_method_in_mro(state, class_name, attr) {
        return Ok(Value::Function(std::sync::Arc::new(def)));
    }
    Err(InterpreterError::AttributeError(format!(
        "type object '{class_name}' has no attribute '{attr}'"
    ))
    .into())
}

/// Walk `class_name`'s MRO looking for `attr` in each ancestor's
/// `class_attrs`. Returns the first match per CPython's MRO rule (most
/// derived wins). Returns `None` if no class in the MRO defines `attr`.
fn lookup_class_attr(state: &InterpreterState, class_name: &str, attr: &str) -> Option<Value> {
    let class = state.classes.get(class_name)?;
    for ancestor_name in &class.mro {
        if let Some(ancestor) = state.classes.get(ancestor_name) {
            if let Some(value) = ancestor.class_attrs.get(attr) {
                return Some(value.clone());
            }
        }
    }
    None
}

/// Receiver context for `super_method_call`: the super proxy's defining
/// class plus the bound instance. Bundled into a struct so the per-call
/// surface stays under the workspace's 4-positional-arg threshold for
/// `pub` functions (see `.claude/rules/rust.md`).
pub struct SuperReceiver<'a> {
    pub defining_class: &'a str,
    pub instance: InstanceValue,
}

/// Call a method via `super()` — find the next method named
/// `method_name` in `recv.instance.class_name`'s MRO starting at the
/// slot AFTER `recv.defining_class`, then run it with the instance as
/// the receiver. CPython's `super().method(...)` dispatches against the
/// original receiver but skips the calling class's own implementation,
/// which is how cooperative multiple inheritance composes correctly.
pub async fn super_method_call(
    state: &mut InterpreterState,
    recv: SuperReceiver<'_>,
    method_name: &str,
    call: CallArgs<'_>,
    tools: &Tools,
) -> Result<(Value, Value), EvalError> {
    let SuperReceiver { defining_class, instance } = recv;
    let Some(class) = state.classes.get(&instance.class_name) else {
        return Err(InterpreterError::Runtime(format!(
            "super(): instance's class '{}' is not registered",
            instance.class_name
        ))
        .into());
    };
    // Find `defining_class`'s position in the MRO, then scan everything
    // after it for the method.
    let start = class.mro.iter().position(|c| c == defining_class).ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!(
            "super(): '{defining_class}' is not in MRO of '{}'",
            instance.class_name
        )))
    })?;
    let mut found = None;
    for ancestor_name in class.mro.iter().skip(start + 1) {
        if let Some(ancestor) = state.classes.get(ancestor_name) {
            if let Some(def) = ancestor.methods.get(method_name) {
                found = Some(def.clone());
                break;
            }
        }
    }
    let Some(def) = found else {
        // CPython's `object` is the implicit base when the MRO is
        // exhausted. We don't model `object` as a registered class,
        // but the default impls of `__setattr__` / `__delattr__` /
        // `__getattribute__` need to do real work — without them the
        // common idiom `super().__setattr__(name, value)` from a
        // user `__setattr__` has nowhere to land. Implement those
        // three slots inline as the object-level defaults.
        return match method_name {
            "__setattr__" => {
                let attr_name = call
                    .positional
                    .first()
                    .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                    .ok_or_else(|| {
                        EvalError::from(InterpreterError::TypeError(
                            "object.__setattr__: first argument must be str".into(),
                        ))
                    })?;
                let value = call.positional.get(1).cloned().ok_or_else(|| {
                    EvalError::from(InterpreterError::TypeError(
                        "object.__setattr__: missing value argument".into(),
                    ))
                })?;
                // Gate the same names the `setattr` builtin and direct `self.x =`
                // path gate — otherwise `super().__setattr__('__class__', x)`
                // plants a blocked-dunder field, an inconsistency that would
                // become load-bearing if any read carve-out is ever added.
                crate::security::validator::validate_attribute(&attr_name)?;
                let inst = instance;
                inst.fields.lock().insert(attr_name.into(), value);
                let updated = Value::Instance(inst);
                if let Some(name) =
                    state.method_frame_stack.last().and_then(|f| f.self_local_name.clone())
                {
                    state.set_variable(&name, updated.clone()).map_err(EvalError::Interpreter)?;
                }
                Ok((Value::None, updated))
            }
            "__delattr__" => {
                let attr_name = call
                    .positional
                    .first()
                    .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                    .ok_or_else(|| {
                        EvalError::from(InterpreterError::TypeError(
                            "object.__delattr__: argument must be str".into(),
                        ))
                    })?;
                crate::security::validator::validate_attribute(&attr_name)?;
                let inst = instance;
                let class_name = inst.class_name.clone();
                if inst.fields.lock().remove(attr_name.as_str()).is_none() {
                    return Err(InterpreterError::AttributeError(format!(
                        "'{class_name}' object has no attribute '{attr_name}'"
                    ))
                    .into());
                }
                let updated = Value::Instance(inst);
                if let Some(name) =
                    state.method_frame_stack.last().and_then(|f| f.self_local_name.clone())
                {
                    state.set_variable(&name, updated.clone()).map_err(EvalError::Interpreter)?;
                }
                Ok((Value::None, updated))
            }
            "__init__" => {
                // `object.__init__` / `BaseException.__init__`: for an
                // exception subclass, `super().__init__(*args)` sets
                // `self.args = args` (which drives `str(self)`); for a
                // plain object it is a no-op that ignores extra args.
                let inst = instance;
                let is_exc =
                    state.classes.get(&inst.class_name).is_some_and(class_mro_is_exception);
                if is_exc {
                    inst.fields
                        .lock()
                        .insert("args".into(), Value::Tuple(call.positional.to_vec()));
                }
                let updated = Value::Instance(inst);
                if let Some(name) =
                    state.method_frame_stack.last().and_then(|f| f.self_local_name.clone())
                {
                    state.set_variable(&name, updated.clone()).map_err(EvalError::Interpreter)?;
                }
                Ok((Value::None, updated))
            }
            _ => Err(InterpreterError::AttributeError(format!(
                "'super' object has no attribute '{method_name}'"
            ))
            .into()),
        };
    };
    // Capture the caller's `self` local-name BEFORE entering call_method
    // (which pushes its own frame). After the parent method mutates its
    // own copy of self, write the result back to the calling method's
    // self variable so its subsequent statements see the mutation —
    // matching CPython's reference semantics through our owned model.
    let caller_self_name = state.method_frame_stack.last().and_then(|f| f.self_local_name.clone());
    let (returned, configured_self) =
        call_method(state, &def, Value::Instance(instance), call, tools).await?;
    if let Some(name) = caller_self_name {
        let _ = state.set_variable(&name, configured_self.clone());
    }
    Ok((returned, configured_self))
}

/// Call a method via class-bound `super()` (inside a classmethod /
/// `__init_subclass__`): find the next classmethod/method named `method_name`
/// in `class_name`'s MRO after `defining_class` and run it with the class as
/// the receiver. When the MRO is exhausted, the object-level defaults for the
/// class-creation hooks (`__init_subclass__` / `__set_name__`) are no-ops,
/// matching CPython — so `super().__init_subclass__(**kwargs)` boilerplate just
/// returns None.
pub async fn super_class_method_call(
    state: &mut InterpreterState,
    defining_class: &str,
    class_name: &str,
    method_name: &str,
    call: CallArgs<'_>,
    tools: &Tools,
) -> EvalResult {
    let Some(class) = state.classes.get(class_name) else {
        return Err(InterpreterError::Runtime(format!(
            "super(): class '{class_name}' is not registered"
        ))
        .into());
    };
    let start = class.mro.iter().position(|c| c == defining_class).ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!(
            "super(): '{defining_class}' is not in MRO of '{class_name}'"
        )))
    })?;
    let mut found = None;
    for ancestor_name in class.mro.iter().skip(start + 1) {
        if let Some(ancestor) = state.classes.get(ancestor_name) {
            let def = ancestor
                .class_methods
                .get(method_name)
                .or_else(|| ancestor.static_methods.get(method_name))
                .or_else(|| ancestor.methods.get(method_name));
            if let Some(def) = def {
                found = Some(def.clone());
                break;
            }
        }
    }
    match found {
        Some(def) => {
            let (returned, _self) =
                call_method(state, &def, Value::Class(class_name.to_string()), call, tools).await?;
            Ok(returned)
        }
        // `super().__new__(cls)` at the object level: the default
        // `object.__new__` — a blank instance of the class being constructed.
        // The `cls` argument names it (defaulting to the running class).
        None if method_name == "__new__" => {
            let target = match call.positional.first() {
                Some(Value::Class(c)) => c.clone(),
                _ => class_name.to_string(),
            };
            Ok(Value::Instance(InstanceValue {
                class_name: target,
                fields: crate::value::shared_fields(BTreeMap::new()),
            }))
        }
        // Object-level defaults: the class-creation hooks are no-ops.
        None if matches!(method_name, "__init_subclass__" | "__set_name__" | "__init__") => {
            Ok(Value::None)
        }
        None => Err(InterpreterError::AttributeError(format!(
            "'super' object has no attribute '{method_name}'"
        ))
        .into()),
    }
}

/// Walk `class_name`'s MRO looking for a method definition. Used by the
/// call path (`instance_method_call`) and by `instantiate` for
/// `__init__` lookup. Returns the first matching `(defining_class,
/// FunctionDef)` so callers can build the per-frame `__class__` binding
/// for zero-arg `super()`.
/// Method or classmethod named `method_name` on `class_name`'s MRO.
fn lookup_method_or_classmethod(
    state: &InterpreterState,
    class_name: &str,
    method_name: &str,
) -> Option<(String, FunctionDef)> {
    if let Some(found) = lookup_method_in_mro(state, class_name, method_name) {
        return Some(found);
    }
    let class = state.classes.get(class_name)?;
    for ancestor_name in &class.mro {
        if let Some(ancestor) = state.classes.get(ancestor_name) {
            if let Some(def) = ancestor.class_methods.get(method_name) {
                return Some((ancestor_name.clone(), def.clone()));
            }
        }
    }
    None
}

pub fn lookup_method_in_mro(
    state: &InterpreterState,
    class_name: &str,
    method_name: &str,
) -> Option<(String, FunctionDef)> {
    let class = state.classes.get(class_name)?;
    for ancestor_name in &class.mro {
        if let Some(ancestor) = state.classes.get(ancestor_name) {
            if let Some(def) = ancestor.methods.get(method_name) {
                return Some((ancestor_name.clone(), def.clone()));
            }
        }
    }
    None
}

/// `type(name, bases, dict)` — dynamic class creation.
pub(crate) fn dynamic_type_new(
    state: &mut InterpreterState,
    name_v: &Value,
    bases_v: &Value,
    dict_v: &Value,
) -> Result<Value, EvalError> {
    let Value::String(name) = name_v else {
        return Err(InterpreterError::TypeError("type() argument 1 must be str".into()).into());
    };
    let class_name = name.to_string();
    let mut bases: Vec<String> = Vec::new();
    let base_items: Vec<Value> = match bases_v {
        Value::Tuple(items) => items.clone(),
        Value::List(l) => l.lock().clone(),
        _ => {
            return Err(
                InterpreterError::TypeError("type() argument 2 must be a tuple".into()).into()
            );
        }
    };
    for b in base_items {
        match b {
            Value::Class(n) => bases.push(n),
            Value::ExceptionType(n) => bases.push(n),
            Value::Type(n) | Value::BuiltinName(n) if n == "object" || n == "type" => {}
            other => {
                return Err(InterpreterError::TypeError(format!(
                    "type() bases must be types, not '{}'",
                    other.type_name()
                ))
                .into());
            }
        }
    }
    let mut class_attrs = BTreeMap::new();
    if let Value::Dict(map) = dict_v {
        for (k, v) in map.lock().iter() {
            if let crate::value::ValueKey::String(s) = k {
                class_attrs.insert(s.to_string(), v.clone());
            }
        }
    } else {
        return Err(InterpreterError::TypeError("type() argument 3 must be a dict".into()).into());
    }
    let mro = build_mro(&class_name, &bases, &state.classes)?;
    let (slots, slot_names) = parse_slots_attr(class_attrs.get("__slots__"));
    state.classes.insert(class_name.clone(), {
        let mut cv = ClassValue::new(class_name.clone());
        cv.class_attrs = class_attrs;
        cv.bases = bases;
        cv.mro = mro;
        cv.slots = slots;
        cv.slot_names = slot_names;
        cv
    });
    Ok(Value::Class(class_name))
}

/// `Meta.__prepare__(name, bases)` → optional initial namespace dict.
async fn invoke_metaclass_prepare(
    state: &mut InterpreterState,
    meta: &str,
    class_name: &str,
    bases: &[String],
    tools: &Tools,
) -> Result<Option<IndexMap<crate::value::ValueKey, Value>>, EvalError> {
    let Some((_, method)) = lookup_method_or_classmethod(state, meta, "__prepare__") else {
        return Ok(None);
    };
    // PEP 3115: `namespace = metaclass.__prepare__(name, bases)` — NOT bound
    // as an instance method; only (name, bases) are passed.
    let name_v = Value::String(class_name.into());
    let bases_t = Value::Tuple(bases.iter().map(|b| Value::Class(b.clone())).collect());
    let empty_kw = IndexMap::new();
    let returned = crate::eval::functions::call_user_function(
        state,
        &method,
        &[name_v, bases_t],
        &empty_kw,
        tools,
    )
    .await?;
    let _ = meta;
    match returned {
        Value::Dict(map) => Ok(Some(map.lock().clone())),
        Value::None => Ok(None),
        other => Err(InterpreterError::TypeError(format!(
            "__prepare__() must return a mapping, not '{}'",
            other.type_name()
        ))
        .into()),
    }
}

/// Call `Meta.__init__(cls, name, bases, namespace)` when present.
async fn invoke_metaclass_init(
    state: &mut InterpreterState,
    class_name: &str,
    meta: &str,
    tools: &Tools,
) -> Result<(), EvalError> {
    let Some((_, method)) = lookup_method_or_classmethod(state, meta, "__init__") else {
        return Ok(());
    };
    let class = state
        .classes
        .get(class_name)
        .cloned()
        .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(class_name)))?;
    let mut ns = IndexMap::new();
    for (k, v) in &class.class_attrs {
        ns.insert(crate::value::ValueKey::String(k.as_str().into()), v.clone());
    }
    let bases_t = Value::Tuple(class.bases.iter().map(|b| Value::Class(b.clone())).collect());
    let name_v = Value::String(class_name.into());
    let ns_v = Value::Dict(crate::value::shared_dict(ns));
    // CPython: Meta.__init__(cls, name, bases, ns) with cls = the new class.
    let cls_v = Value::Class(class_name.to_string());
    let call = crate::eval::functions::CallArgs {
        positional: &[name_v, bases_t, ns_v],
        keyword: &IndexMap::new(),
    };
    let _ = call_method(state, &method, cls_v, call, tools).await?;
    let _ = meta; // used for MRO lookup above
    Ok(())
}

/// Call `Meta.__new__(Meta, name, bases, namespace)` when present.
async fn invoke_metaclass_new(
    state: &mut InterpreterState,
    class_name: &str,
    meta: &str,
    tools: &Tools,
) -> Result<(), EvalError> {
    let Some((_, method)) = lookup_method_or_classmethod(state, meta, "__new__") else {
        return Ok(());
    };
    let class = state
        .classes
        .get(class_name)
        .cloned()
        .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(class_name)))?;
    let mut ns = IndexMap::new();
    for (k, v) in &class.class_attrs {
        ns.insert(crate::value::ValueKey::String(k.as_str().into()), v.clone());
    }
    let bases_t = Value::Tuple(class.bases.iter().map(|b| Value::Class(b.clone())).collect());
    let name_v = Value::String(class_name.into());
    let ns_v = Value::Dict(crate::value::shared_dict(ns));
    let meta_v = Value::Class(meta.to_string());
    // call_method prepends the receiver as `self`/`cls`.
    let call = crate::eval::functions::CallArgs {
        positional: &[name_v, bases_t, ns_v],
        keyword: &IndexMap::new(),
    };
    let (returned, _) = call_method(state, &method, meta_v, call, tools).await?;
    match returned {
        Value::Class(n) => {
            state.set_variable(class_name, Value::Class(n)).map_err(EvalError::Interpreter)?;
        }
        Value::None => {}
        other => {
            state.set_variable(class_name, other).map_err(EvalError::Interpreter)?;
        }
    }
    Ok(())
}

/// Parse class-body `__slots__` into (enabled, names).
fn parse_slots_attr(attr: Option<&Value>) -> (bool, Vec<String>) {
    let Some(attr) = attr else {
        return (false, Vec::new());
    };
    let names = match attr {
        Value::String(s) => vec![s.to_string()],
        Value::Tuple(items) => items
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.to_string()),
                _ => None,
            })
            .collect(),
        Value::List(shared) => {
            let guard = shared.lock();
            guard
                .iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.to_string()),
                    _ => None,
                })
                .collect()
        }
        _ => return (false, Vec::new()),
    };
    (true, names)
}
