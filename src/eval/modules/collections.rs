// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `collections` module — Counter, deque,
//! defaultdict, OrderedDict.

use std::collections::VecDeque;

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{control_flow::iterate_value, literals::value_to_key},
    value::{Value, ValueKey},
};

/// Whether `collections` provides a callable named `name`.
pub fn has_function(name: &str) -> bool {
    matches!(name, "Counter" | "deque" | "defaultdict" | "OrderedDict" | "namedtuple")
}

/// Invoke a `collections` callable.
pub fn call(func: &str, args: &[Value], kwargs: &IndexMap<String, Value>) -> EvalResult {
    match func {
        // `Counter(iterable)` tallies element occurrences (Track B3).
        // `Counter(apple=3, banana=2)` seeds counts from keyword args
        // — CPython's documented constructor surface.
        "Counter" => {
            let mut counts: IndexMap<ValueKey, Value> = IndexMap::new();
            if let Some(arg) = args.first() {
                if let Value::Dict(map) | Value::Counter(map) = arg {
                    for (k, v) in map {
                        counts.insert(k.clone(), v.clone());
                    }
                } else {
                    for item in iterate_value(arg)? {
                        let key = value_to_key(&item)?;
                        let entry = counts.entry(key).or_insert(Value::Int(0));
                        if let Value::Int(n) = entry {
                            *n += 1;
                        }
                    }
                }
            }
            for (key, value) in kwargs {
                counts.insert(ValueKey::String(key.as_str().into()), value.clone());
            }
            Ok(Value::Counter(counts))
        }
        // `deque([iterable, [maxlen]])` — double-ended queue.
        "deque" => {
            let items: VecDeque<Value> = match args.first() {
                None | Some(Value::None) => VecDeque::new(),
                Some(arg) => iterate_value(arg)?.into_iter().collect(),
            };
            let maxlen = match args.get(1) {
                None | Some(Value::None) => None,
                Some(Value::Int(n)) => Some(usize::try_from(*n).map_err(|_| {
                    EvalError::from(InterpreterError::ValueError(
                        "deque maxlen must be non-negative".into(),
                    ))
                })?),
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "deque maxlen must be an integer or None (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            // Apply maxlen by trimming from the front.
            let mut deque = items;
            if let Some(cap) = maxlen {
                while deque.len() > cap {
                    deque.pop_front();
                }
            }
            Ok(Value::Deque { items: deque, maxlen })
        }
        // `defaultdict(factory[, mapping_or_iterable])` — dict with
        // missing-key synthesis. Factory must be callable.
        "defaultdict" => {
            let factory = args.first().cloned().unwrap_or(Value::None);
            // Accept Function/Lambda/Class/None plus the typed
            // BuiltinName variant that bare names like `int`/`list`/
            // `dict` resolve to. invoke_factory in eval/names.rs
            // handles the BuiltinName-to-empty-container call.
            if !matches!(
                factory,
                Value::Function(_)
                    | Value::Lambda(_)
                    | Value::Class(_)
                    | Value::None
                    | Value::BuiltinName(_)
            ) {
                return Err(InterpreterError::TypeError(format!(
                    "first argument must be callable or None (got '{}')",
                    factory.type_name()
                ))
                .into());
            }
            let mut items: IndexMap<ValueKey, Value> = IndexMap::new();
            if let Some(arg) = args.get(1) {
                if let Value::Dict(map) = arg {
                    for (k, v) in map {
                        items.insert(k.clone(), v.clone());
                    }
                } else {
                    for pair in iterate_value(arg)? {
                        let pair_items = iterate_value(&pair)?;
                        if pair_items.len() != 2 {
                            return Err(InterpreterError::ValueError(
                                "defaultdict iterable elements must be 2-tuples".into(),
                            )
                            .into());
                        }
                        let key = value_to_key(&pair_items[0])?;
                        items.insert(key, pair_items[1].clone());
                    }
                }
            }
            Ok(Value::DefaultDict(Box::new(crate::value::DefaultDictData { items, factory })))
        }
        // `OrderedDict([mapping_or_iterable])` — Track E thin shim.
        // CPython's dict has been insertion-ordered since 3.7, so we
        // return a regular Dict. `move_to_end` is exposed via dict's
        // method dispatch (Track E batch 3 addition).
        "OrderedDict" => {
            let mut entries: IndexMap<ValueKey, Value> = IndexMap::new();
            if let Some(arg) = args.first() {
                if let Value::Dict(map) | Value::Counter(map) = arg {
                    for (k, v) in map {
                        entries.insert(k.clone(), v.clone());
                    }
                } else {
                    for pair in iterate_value(arg)? {
                        let pair_items = iterate_value(&pair)?;
                        if pair_items.len() != 2 {
                            return Err(InterpreterError::ValueError(
                                "OrderedDict iterable elements must be 2-tuples".into(),
                            )
                            .into());
                        }
                        let key = value_to_key(&pair_items[0])?;
                        entries.insert(key, pair_items[1].clone());
                    }
                }
            }
            Ok(Value::Dict(entries))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'collections' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// Track E batch 3: `namedtuple(name, fields)` — synthesises a class
/// whose `__init__` binds positional args to the named fields. Field
/// access via attribute works; subscript (`nt[i]`) is handled in
/// `eval_subscript`; iteration / `len` use the `_fields` class attr
/// via `op::namedtuple_items` (field order). `_fields` also drives
/// PEP 634 `__match_args__`.
pub(crate) fn call_namedtuple_with_state(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
) -> EvalResult {
    let class_name = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => {
            return Err(InterpreterError::TypeError(
                "namedtuple() first argument must be the class name".into(),
            )
            .into());
        }
    };
    let fields: Vec<String> = match args.get(1) {
        Some(Value::String(s)) => {
            // Accept space-separated or comma-separated field strings.
            s.split(|c: char| c.is_whitespace() || c == ',')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        }
        Some(Value::List(items)) => items
            .lock()
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.to_string()),
                _ => None,
            })
            .collect(),
        Some(Value::Tuple(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.to_string()),
                _ => None,
            })
            .collect(),
        _ => {
            return Err(InterpreterError::TypeError(
                "namedtuple() second argument must be a sequence of field names".into(),
            )
            .into());
        }
    };
    // Build a synthetic __init__ method that binds each positional
    // arg to the corresponding field. We do this by registering the
    // function bodies under qualified keys and adding the class to
    // state.classes.
    use std::collections::BTreeMap;

    use crate::value::{ClassValue, FunctionDef, FunctionParams, Param};
    let init_params = FunctionParams {
        args: std::iter::once(Param { name: "self".to_string() })
            .chain(fields.iter().map(|f| Param { name: f.clone() }))
            .collect(),
        defaults: Vec::new(),
        default_values: Vec::new(),
        vararg: None,
        kwonlyargs: Vec::new(),
        kw_defaults: Vec::new(),
        kw_default_values: Vec::new(),
        kwarg: None,
    };
    // Build the body: self.field_n = field_n for each field.
    use rustpython_parser::{
        ast::{self as ast_, Expr, ExprAttribute, ExprContext, ExprName, Stmt, StmtAssign},
        text_size::TextRange,
    };
    let body: Vec<Stmt> = fields
        .iter()
        .map(|field| {
            let target = Expr::Attribute(ExprAttribute {
                value: Box::new(Expr::Name(ExprName {
                    id: ast_::Identifier::new("self"),
                    ctx: ExprContext::Store,
                    range: TextRange::default(),
                })),
                attr: ast_::Identifier::new(field.clone()),
                ctx: ExprContext::Store,
                range: TextRange::default(),
            });
            let value = Expr::Name(ExprName {
                id: ast_::Identifier::new(field.clone()),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            });
            Stmt::Assign(StmtAssign {
                targets: vec![target],
                value: Box::new(value),
                type_comment: None,
                range: TextRange::default(),
            })
        })
        .collect();
    let init_key = format!("{class_name}.__init__");
    state.function_bodies.insert(init_key.clone(), std::sync::Arc::new(body));
    let init_def = FunctionDef {
        name: init_key,
        params: init_params,
        closure: BTreeMap::new(),
        source: String::new(),
        nonlocal_names: Vec::new(),
        // Synthesized __init__ does not yield.
        is_generator: false,
        nonlocal_cell_id: None,
        // Synthesized namedtuple __init__ assigns to `self.<field>`
        // (attribute set, no local binding), so no checkpoint-tracked
        // names. Globals likewise empty.
        assigned_names: Vec::new(),
        global_names: Vec::new(),
        // Synthesized methods carry empty closures; the flag is
        // immaterial here.
        is_module_level: false,
    };
    let mut methods: BTreeMap<String, FunctionDef> = BTreeMap::new();
    methods.insert("__init__".to_string(), init_def);

    // Synthesize `_asdict(self)` → `{<field>: self.<field>, ...}`.
    // Matches CPython's namedtuple._asdict, which returns a regular
    // dict in field order (no longer an OrderedDict since 3.8).
    use rustpython_parser::ast::{ExprDict, StmtReturn};
    let asdict_body = vec![Stmt::Return(StmtReturn {
        value: Some(Box::new(Expr::Dict(ExprDict {
            keys: fields
                .iter()
                .map(|f| {
                    Some(Expr::Constant(ast_::ExprConstant {
                        value: ast_::Constant::Str(f.clone()),
                        kind: None,
                        range: TextRange::default(),
                    }))
                })
                .collect(),
            values: fields
                .iter()
                .map(|f| {
                    Expr::Attribute(ExprAttribute {
                        value: Box::new(Expr::Name(ExprName {
                            id: ast_::Identifier::new("self"),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                        })),
                        attr: ast_::Identifier::new(f.clone()),
                        ctx: ExprContext::Load,
                        range: TextRange::default(),
                    })
                })
                .collect(),
            range: TextRange::default(),
        }))),
        range: TextRange::default(),
    })];
    let asdict_key = format!("{class_name}._asdict");
    state.function_bodies.insert(asdict_key.clone(), std::sync::Arc::new(asdict_body));
    methods.insert(
        "_asdict".to_string(),
        FunctionDef {
            name: asdict_key,
            params: FunctionParams {
                args: vec![Param { name: "self".to_string() }],
                defaults: Vec::new(),
                default_values: Vec::new(),
                vararg: None,
                kwonlyargs: Vec::new(),
                kw_defaults: Vec::new(),
                kw_default_values: Vec::new(),
                kwarg: None,
            },
            closure: BTreeMap::new(),
            source: String::new(),
            nonlocal_names: Vec::new(),
            is_generator: false,
            nonlocal_cell_id: None,
            // Synthesized `_asdict` returns a dict literal with no
            // local bindings.
            assigned_names: Vec::new(),
            global_names: Vec::new(),
            is_module_level: false,
        },
    );

    // Class attributes: _fields tuple, __match_args__ tuple (so PEP
    // 634 class patterns work on namedtuple instances).
    let mut class_attrs: BTreeMap<String, Value> = BTreeMap::new();
    let fields_tuple =
        Value::Tuple(fields.iter().map(|s| Value::String(s.as_str().into())).collect());
    class_attrs.insert("_fields".to_string(), fields_tuple.clone());
    class_attrs.insert("__match_args__".to_string(), fields_tuple);
    let class_name_str = class_name.to_string();
    state.classes.insert(
        class_name_str.clone(),
        ClassValue {
            name: class_name_str.clone(),
            methods,
            class_attrs,
            bases: Vec::new(),
            mro: vec![class_name_str.clone()],
            properties: BTreeMap::new(),
            static_methods: BTreeMap::new(),
            class_methods: BTreeMap::new(),
            enum_kind: None,
            annotations: Vec::new(),
            dataclass_fields: None,
        },
    );
    Ok(Value::Class(class_name_str))
}

/// `collections` module registration. Handles the namedtuple special-
/// case (it needs `state` to synthesize a class) inside the trait
/// dispatch so the registry's [`Module::call`] surface stays uniform.
pub struct CollectionsModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for CollectionsModule {
    fn name(&self) -> &'static str {
        "collections"
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        kwargs: &indexmap::IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        match func {
            "namedtuple" => call_namedtuple_with_state(state, args),
            _ => call(func, args, kwargs),
        }
    }
}
