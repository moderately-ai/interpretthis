// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `dataclasses` module.
//!
//! Implements the bare `@dataclass` decorator + `field(default=...,
//! default_factory=...)` helper. The decorator reads the target class's
//! annotated attributes (in declaration order, captured at class-eval
//! time as [`ClassValue::annotations`]) and computes a
//! [`DataclassField`] list, then writes it onto the class via
//! [`ClassValue::dataclass_fields`]. The synthesized `__init__` /
//! `__repr__` / `__eq__` / `__match_args__` are realised at use-time
//! rather than by injecting Python source — the evaluator intercepts
//! instance construction, equality, and repr against the
//! `dataclass_fields` table.
//!
//! The decorator path runs through
//! [`crate::eval::classes::apply_decorator`], which is extended to
//! recognise `ModuleFunction { module: "dataclasses", name: "dataclass" }`
//! and call into [`apply_dataclass`].

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    value::{DataclassField, InstanceValue, Value, shared_list},
};

/// Sentinel: every `dataclasses.field(...)` returns one of these,
/// recognised by [`apply_dataclass`] when scanning the class body's
/// existing class attributes for per-field configuration.
const FIELD_SENTINEL: &str = "__interpretthis_dataclasses_field__";

pub fn has_function(name: &str) -> bool {
    matches!(name, "dataclass" | "field" | "is_dataclass" | "fields" | "asdict" | "astuple")
}

/// Call into a `dataclasses.<func>` module function. Unlike most module
/// modules in this directory, `dataclass` itself is invoked as a
/// decorator and routed through [`crate::eval::classes::apply_decorator`]
/// rather than this `call` path; the entry here handles the call-shape
/// `@dataclass()` (no-arg call returning a partial decorator) and the
/// helpers that DO take regular arguments (`field`, `is_dataclass`,
/// `fields`, `asdict`, `astuple`).
pub fn call(
    state: &mut InterpreterState,
    func: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
) -> EvalResult {
    match func {
        // `@dataclass()` (called form). With no positional args, returns
        // a sentinel that the decorator path recognises and applies to
        // the class. We model this as a ModuleFunction pointing back at
        // `dataclass` — calling it on a class is identical to bare
        // `@dataclass`. Keyword arguments are validated but currently
        // accepted as defaults (eq=True, repr=True, init=True) since we
        // do not yet support frozen/order in this slice.
        "dataclass" => {
            // The call form may receive the class directly (`dataclass(C)`
            // is equivalent to `@dataclass class C`). Detect that and
            // apply immediately.
            if let Some(Value::Class(class_name)) = args.first() {
                apply_dataclass(state, class_name, kwargs)?;
                return Ok(Value::Class(class_name.clone()));
            }
            // `@dataclass(frozen=True, …)` — return a Partial that carries
            // the kwargs so the decorator pipeline can apply them.
            if !kwargs.is_empty() {
                return Ok(Value::Partial(Box::new(crate::value::PartialData {
                    func: Value::ModuleFunction {
                        module: "dataclasses".to_string(),
                        name: "dataclass".to_string(),
                    },
                    args: Vec::new(),
                    keywords: kwargs.clone(),
                })));
            }
            // Bare `@dataclass` — ModuleFunction handle applied later.
            Ok(Value::ModuleFunction {
                module: "dataclasses".to_string(),
                name: "dataclass".to_string(),
            })
        }
        "field" => Ok(build_field_sentinel(kwargs)),
        "is_dataclass" => {
            let target = args.first().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "is_dataclass() missing required argument".into(),
                ))
            })?;
            let class_name = match target {
                Value::Class(name) => name.as_str(),
                Value::Instance(inst) => inst.class_name.as_str(),
                _ => return Ok(Value::Bool(false)),
            };
            Ok(Value::Bool(
                state.classes.get(class_name).is_some_and(|class| class.dataclass_fields.is_some()),
            ))
        }
        "fields" => {
            let class_name = resolve_class_name(args.first())?;
            let class = state
                .classes
                .get(&class_name)
                .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&class_name)))?;
            let fields = class.dataclass_fields.as_ref().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(format!(
                    "fields() requires a dataclass instance or class; '{class_name}' is not a dataclass"
                )))
            })?;
            // Return a tuple of small dicts {name, default} per field —
            // a CPython `Field` object has more methods, but the dict
            // shape is the common consumer pattern.
            let items = fields
                .iter()
                .map(|f| {
                    let mut entry = IndexMap::new();
                    entry.insert(
                        crate::value::ValueKey::String("name".into()),
                        Value::String(f.name.as_str().into()),
                    );
                    entry.insert(
                        crate::value::ValueKey::String("default".into()),
                        f.default.clone().unwrap_or(Value::None),
                    );
                    Value::Dict(entry)
                })
                .collect();
            Ok(Value::Tuple(items))
        }
        "asdict" => match args.first() {
            Some(Value::Instance(inst)) => asdict_recursive(state, inst),
            _ => Err(InterpreterError::TypeError(
                "asdict() should be called on dataclass instances".into(),
            )
            .into()),
        },
        "astuple" => match args.first() {
            Some(Value::Instance(inst)) => astuple_recursive(state, inst),
            _ => Err(InterpreterError::TypeError(
                "astuple() should be called on dataclass instances".into(),
            )
            .into()),
        },
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'dataclasses' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// Recursive `dataclasses.asdict` — replicates CPython's traversal
/// through nested dataclasses, lists, tuples, and dicts. Non-dataclass
/// values are left as-is (matching CPython's `copy.deepcopy`-shaped
/// fallback for terminal nodes).
fn asdict_recursive(state: &InterpreterState, inst: &InstanceValue) -> EvalResult {
    let class = state
        .classes
        .get(&inst.class_name)
        .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&inst.class_name)))?;
    let fields = class.dataclass_fields.as_ref().ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!(
            "asdict() should be called on dataclass instances; '{}' is not a dataclass",
            inst.class_name
        )))
    })?;
    let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
    let mut out: IndexMap<crate::value::ValueKey, Value> = IndexMap::new();
    for name in field_names {
        let raw = inst.fields.get(&name).cloned().unwrap_or(Value::None);
        let converted = convert_for_asdict(state, &raw)?;
        out.insert(crate::value::ValueKey::String(name.as_str().into()), converted);
    }
    Ok(Value::Dict(out))
}

fn convert_for_asdict(state: &InterpreterState, value: &Value) -> EvalResult {
    match value {
        Value::Instance(inner) => {
            if state.classes.get(&inner.class_name).is_some_and(|c| c.dataclass_fields.is_some()) {
                asdict_recursive(state, inner)
            } else {
                Ok(value.clone())
            }
        }
        Value::List(items) => {
            // Snapshot under the lock so the recursive
            // `convert_for_asdict` call doesn't hold a guard across its
            // own potential re-locking of the same SharedList.
            let snapshot = items.lock().clone();
            let mut out = Vec::with_capacity(snapshot.len());
            for item in &snapshot {
                out.push(convert_for_asdict(state, item)?);
            }
            Ok(Value::List(shared_list(out)))
        }
        Value::Tuple(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(convert_for_asdict(state, item)?);
            }
            Ok(Value::Tuple(out))
        }
        Value::Dict(items) => {
            let mut out: IndexMap<crate::value::ValueKey, Value> = IndexMap::new();
            for (key, val) in items {
                out.insert(key.clone(), convert_for_asdict(state, val)?);
            }
            Ok(Value::Dict(out))
        }
        other => Ok(other.clone()),
    }
}

/// Recursive `dataclasses.astuple` — same traversal as asdict but
/// fields collapse into a tuple in declaration order.
fn astuple_recursive(state: &InterpreterState, inst: &InstanceValue) -> EvalResult {
    let class = state
        .classes
        .get(&inst.class_name)
        .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(&inst.class_name)))?;
    let fields = class.dataclass_fields.as_ref().ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!(
            "astuple() should be called on dataclass instances; '{}' is not a dataclass",
            inst.class_name
        )))
    })?;
    let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
    let mut out = Vec::with_capacity(field_names.len());
    for name in field_names {
        let raw = inst.fields.get(&name).cloned().unwrap_or(Value::None);
        out.push(convert_for_astuple(state, &raw)?);
    }
    Ok(Value::Tuple(out))
}

fn convert_for_astuple(state: &InterpreterState, value: &Value) -> EvalResult {
    match value {
        Value::Instance(inner) => {
            if state.classes.get(&inner.class_name).is_some_and(|c| c.dataclass_fields.is_some()) {
                astuple_recursive(state, inner)
            } else {
                Ok(value.clone())
            }
        }
        Value::List(items) => {
            // Snapshot under the lock so the recursive
            // `convert_for_astuple` call doesn't hold a guard across its
            // own potential re-locking of the same SharedList.
            let snapshot = items.lock().clone();
            let mut out = Vec::with_capacity(snapshot.len());
            for item in &snapshot {
                out.push(convert_for_astuple(state, item)?);
            }
            Ok(Value::List(shared_list(out)))
        }
        Value::Tuple(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(convert_for_astuple(state, item)?);
            }
            Ok(Value::Tuple(out))
        }
        Value::Dict(items) => {
            let mut out: IndexMap<crate::value::ValueKey, Value> = IndexMap::new();
            for (key, val) in items {
                out.insert(key.clone(), convert_for_astuple(state, val)?);
            }
            Ok(Value::Dict(out))
        }
        other => Ok(other.clone()),
    }
}

fn resolve_class_name(arg: Option<&Value>) -> Result<String, EvalError> {
    match arg {
        Some(Value::Class(name)) => Ok(name.clone()),
        Some(Value::Instance(inst)) => Ok(inst.class_name.clone()),
        _ => Err(InterpreterError::TypeError(
            "fields() expects a dataclass class or instance".into(),
        )
        .into()),
    }
}

/// Apply the `@dataclass` decorator to `class_name`, mutating the class
/// in the registry to record its [`DataclassField`] list and to install
/// `__match_args__` (a tuple of field names) so PEP-634 class patterns
/// work without further plumbing.
///
/// Honours `frozen=` and `order=` kwargs. `slots=` / `kw_only=` are
/// accepted but no-op (see tickets).
pub(crate) fn apply_dataclass(
    state: &mut InterpreterState,
    class_name: &str,
    kwargs: &IndexMap<String, Value>,
) -> Result<(), EvalError> {
    let frozen = kwargs.get("frozen").is_some_and(Value::is_truthy);
    let order = kwargs.get("order").is_some_and(Value::is_truthy);
    let class = state
        .classes
        .get(class_name)
        .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(class_name)))?;
    let annotations = class.annotations.clone();
    let mut fields: Vec<DataclassField> = Vec::with_capacity(annotations.len());
    for name in &annotations {
        // A class attribute matching the annotation name supplies the
        // default. If the attribute is a `field(...)` sentinel, unpack it
        // into the field's flag set + defaults.
        let class_attr = class.class_attrs.get(name).cloned();
        let field = build_field(name.clone(), class_attr);
        fields.push(field);
    }

    // CPython rule: a non-default field cannot follow a default field —
    // the `__init__` signature would be ambiguous. Match the exact
    // error wording so call sites that catch on substring keep working.
    let mut seen_default = None;
    for field in &fields {
        if field.default.is_some() || field.default_factory.is_some() {
            seen_default = Some(field.name.clone());
        } else if let Some(prior) = &seen_default {
            return Err(InterpreterError::TypeError(format!(
                "non-default argument '{}' follows default argument '{}'",
                field.name, prior
            ))
            .into());
        }
    }

    // Install __match_args__ as a tuple of the field names so PEP 634
    // class patterns work on dataclass instances without further work.
    let match_args = Value::Tuple(
        fields.iter().filter(|f| f.init).map(|f| Value::String(f.name.as_str().into())).collect(),
    );

    let class_mut = state
        .classes
        .get_mut(class_name)
        .ok_or_else(|| EvalError::from(InterpreterError::name_not_defined(class_name)))?;
    class_mut.class_attrs.insert("__match_args__".to_string(), match_args);
    class_mut.dataclass_fields = Some(fields);
    class_mut.frozen = frozen;
    class_mut.order = order;
    Ok(())
}

/// Translate a (possibly-`field()`-sentinel) class-attribute value into
/// a per-field [`DataclassField`] entry. A plain literal becomes the
/// `default`; a `field(...)` dict carries the per-field flags.
fn build_field(name: String, class_attr: Option<Value>) -> DataclassField {
    let mut default = None;
    let mut default_factory = None;
    let mut init = true;
    let mut repr = true;
    let mut compare = true;
    if let Some(value) = class_attr {
        if let Some(sentinel) = unpack_field_sentinel(&value) {
            default = sentinel.default;
            default_factory = sentinel.default_factory;
            init = sentinel.init;
            repr = sentinel.repr;
            compare = sentinel.compare;
        } else {
            default = Some(value);
        }
    }
    DataclassField { name, default, default_factory, init, repr, compare }
}

/// Build the dict returned by `field(...)`. A small `Value::Dict` keyed
/// by a sentinel discriminator so [`unpack_field_sentinel`] can identify
/// it without confusion vs a regular user dict default.
fn build_field_sentinel(kwargs: &IndexMap<String, Value>) -> Value {
    let mut dict: IndexMap<crate::value::ValueKey, Value> = IndexMap::new();
    dict.insert(
        crate::value::ValueKey::String("__interpretthis_kind__".into()),
        Value::String(FIELD_SENTINEL.into()),
    );
    if let Some(default) = kwargs.get("default") {
        dict.insert(crate::value::ValueKey::String("default".into()), default.clone());
    }
    if let Some(default_factory) = kwargs.get("default_factory") {
        dict.insert(
            crate::value::ValueKey::String("default_factory".into()),
            default_factory.clone(),
        );
    }
    for (key, value) in kwargs {
        if matches!(key.as_str(), "init" | "repr" | "compare") {
            dict.insert(crate::value::ValueKey::String(key.as_str().into()), value.clone());
        }
    }
    Value::Dict(dict)
}

/// Unpacked shape of a `field(...)` sentinel — the named-parameter
/// payload [`build_field`] reads when translating a class attribute
/// into a [`DataclassField`].
struct FieldSentinel {
    default: Option<Value>,
    default_factory: Option<Value>,
    init: bool,
    repr: bool,
    compare: bool,
}

/// Decode a `field(...)` sentinel dict. `None` if `value` is not a
/// field sentinel (a regular literal default flows through unchanged).
fn unpack_field_sentinel(value: &Value) -> Option<FieldSentinel> {
    let Value::Dict(dict) = value else { return None };
    let kind = dict.get(&crate::value::ValueKey::String("__interpretthis_kind__".into()))?;
    let Value::String(kind_str) = kind else { return None };
    if kind_str != FIELD_SENTINEL {
        return None;
    }
    let default = dict.get(&crate::value::ValueKey::String("default".into())).cloned();
    let default_factory =
        dict.get(&crate::value::ValueKey::String("default_factory".into())).cloned();
    let init = dict
        .get(&crate::value::ValueKey::String("init".into()))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let repr = dict
        .get(&crate::value::ValueKey::String("repr".into()))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let compare = dict
        .get(&crate::value::ValueKey::String("compare".into()))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    Some(FieldSentinel { default, default_factory, init, repr, compare })
}

/// `dataclasses` module registration.
pub struct DataclassesModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for DataclassesModule {
    fn name(&self) -> &'static str {
        "dataclasses"
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        kwargs: &IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        call(state, func, args, kwargs)
    }
}
