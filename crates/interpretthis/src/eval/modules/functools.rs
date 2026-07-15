// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `functools` module.
//!
//! Supports `reduce` (call-back into the evaluator's `call_user_function`
//! / `call_lambda` for each pair) and `wraps` (stamps the wrapped function's
//! `__name__` onto the wrapper via the `wraps_name` override).
//!
//! Also: `partial`, `cmp_to_key`. `lru_cache` / `cache` / `singledispatch`
//! remain open (see tickets).

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::control_flow::iterate_value,
    state::InterpreterState,
    tools::Tools,
    value::{ClassValue, InstanceValue, Value},
};

/// Marker class for objects returned by `cmp_to_key` factories.
pub const CMP_KEY_CLASS: &str = "functools.CmpKey";

pub fn has_function(name: &str) -> bool {
    matches!(
        name,
        "wraps"
            | "reduce"
            | "partial"
            | "cmp_to_key"
            | "_cmp_key"
            | "lru_cache"
            | "cache"
            | "_lru_wrap"
            | "total_ordering"
    )
}

fn parse_maxsize(
    positional: Option<&Value>,
    kwargs: &IndexMap<String, Value>,
) -> Result<Option<usize>, EvalError> {
    if let Some(v) = kwargs.get("maxsize") {
        return match v {
            Value::None => Ok(None),
            Value::Int(n) if *n < 0 => Ok(None),
            Value::Int(n) => Ok(Some(usize::try_from(*n).unwrap_or(usize::MAX))),
            other => Err(InterpreterError::TypeError(format!(
                "maxsize must be an integer or None, not '{}'",
                other.type_name()
            ))
            .into()),
        };
    }
    match positional {
        Some(Value::None) => Ok(None),
        Some(Value::Int(n)) if *n < 0 => Ok(None),
        Some(Value::Int(n)) => Ok(Some(usize::try_from(*n).unwrap_or(usize::MAX))),
        None => Ok(Some(128)), // CPython default
        Some(other) => Err(InterpreterError::TypeError(format!(
            "maxsize must be an integer or None, not '{}'",
            other.type_name()
        ))
        .into()),
    }
}

pub(crate) fn make_lru_cache_pub(func: Value, maxsize: Option<usize>) -> Value {
    make_lru_cache(func, maxsize)
}

fn make_lru_cache(func: Value, maxsize: Option<usize>) -> Value {
    Value::LruCache(std::sync::Arc::new(crate::value::LruCacheData {
        func,
        maxsize,
        cache: parking_lot::Mutex::new(IndexMap::new()),
    }))
}

pub async fn call(
    state: &mut InterpreterState,
    func: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> EvalResult {
    match func {
        "partial" => {
            // `partial(func, *args, **kwargs)` returns a callable
            // that forwards to func with the bound args/kwargs
            // prepended/merged. CPython exposes `.func`, `.args`,
            // `.keywords` attributes on the returned partial; we
            // expose the same via the Value::Partial variant.
            let Some(target) = args.first().cloned() else {
                return Err(InterpreterError::TypeError(
                    "partial() requires at least one positional argument".into(),
                )
                .into());
            };
            Ok(Value::Partial(Box::new(crate::value::PartialData {
                func: target,
                args: args[1..].to_vec(),
                keywords: kwargs.clone(),
            })))
        }
        "lru_cache" => {
            // Forms:
            //   @lru_cache          → ModuleFunction applied as decorator
            //   @lru_cache()        → maxsize=128 factory
            //   @lru_cache(maxsize=n)
            //   @lru_cache(None)    → unbounded
            //   lru_cache(f)        → wrap f directly
            if let Some(func) = args.first() {
                if matches!(func, Value::Function(_) | Value::Lambda(_) | Value::Partial(_)) {
                    let maxsize = parse_maxsize(args.get(1), kwargs)?;
                    return Ok(make_lru_cache(func.clone(), maxsize));
                }
                // lru_cache(None) → unbounded factory
                if matches!(func, Value::None) {
                    return Ok(Value::Partial(Box::new(crate::value::PartialData {
                        func: Value::ModuleFunction {
                            module: "functools".into(),
                            name: "_lru_wrap".into(),
                        },
                        args: vec![Value::None],
                        keywords: IndexMap::new(),
                    })));
                }
            }
            let maxsize = parse_maxsize(None, kwargs)?;
            // Factory decorator: bind maxsize, wait for function.
            Ok(Value::Partial(Box::new(crate::value::PartialData {
                func: Value::ModuleFunction {
                    module: "functools".into(),
                    name: "_lru_wrap".into(),
                },
                args: vec![Value::Int(
                    maxsize.map_or(-1, |n| i64::try_from(n).unwrap_or(i64::MAX)),
                )],
                keywords: IndexMap::new(),
            })))
        }
        "cache" => {
            // @cache ≡ @lru_cache(maxsize=None)
            if let Some(func) = args.first() {
                return Ok(make_lru_cache(func.clone(), None));
            }
            Ok(Value::Partial(Box::new(crate::value::PartialData {
                func: Value::ModuleFunction {
                    module: "functools".into(),
                    name: "_lru_wrap".into(),
                },
                args: vec![Value::None],
                keywords: IndexMap::new(),
            })))
        }
        "_lru_wrap" => {
            // Internal: _lru_wrap(maxsize_sentinel, func)
            // maxsize: Int(n), None or Int(-1) => unbounded
            let maxsize = match args.first() {
                Some(Value::None) | None => None,
                Some(Value::Int(n)) if *n < 0 => None,
                Some(Value::Int(n)) => Some(usize::try_from(*n).unwrap_or(usize::MAX)),
                _ => Some(128),
            };
            let func = args.get(1).cloned().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "lru_cache decorator requires a function".into(),
                ))
            })?;
            Ok(make_lru_cache(func, maxsize))
        }
        "total_ordering" => {
            // Class decorator: flag the class so op::compare derives the
            // missing ordering operators from the one it defines plus __eq__.
            let Some(Value::Class(class_name)) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "total_ordering() argument must be a class".into(),
                )
                .into());
            };
            let class = state.classes.get(class_name).ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(format!(
                    "total_ordering: unknown class '{class_name}'"
                )))
            })?;
            // CPython requires __eq__ and at least one ordering root.
            let has_root = ["__lt__", "__le__", "__gt__", "__ge__"]
                .iter()
                .any(|m| class.methods.contains_key(*m));
            if !has_root {
                return Err(InterpreterError::ValueError(
                    "must define at least one ordering operation: < > <= >=".into(),
                )
                .into());
            }
            if let Some(class) = state.classes.get_mut(class_name) {
                class.total_ordering = true;
            }
            Ok(Value::Class(class_name.clone()))
        }
        "cmp_to_key" => {
            // Returns a key= factory: key(obj) wraps obj for cmp-based sort.
            let Some(cmp) = args.first().cloned() else {
                return Err(InterpreterError::TypeError(
                    "cmp_to_key() missing required argument: 'mycmp'".into(),
                )
                .into());
            };
            ensure_cmp_key_class(state);
            Ok(Value::Partial(Box::new(crate::value::PartialData {
                func: Value::ModuleFunction { module: "functools".into(), name: "_cmp_key".into() },
                args: vec![cmp],
                keywords: IndexMap::new(),
            })))
        }
        "_cmp_key" => {
            // Internal: _cmp_key(cmp, obj) -> CmpKey instance.
            let cmp = args.first().cloned().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError("_cmp_key() missing cmp".into()))
            })?;
            let obj = args.get(1).cloned().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError("_cmp_key() missing obj".into()))
            })?;
            ensure_cmp_key_class(state);
            let mut fields = std::collections::BTreeMap::new();
            fields.insert("cmp".into(), cmp);
            fields.insert("obj".into(), obj);
            Ok(Value::Instance(InstanceValue {
                class_name: CMP_KEY_CLASS.into(),
                fields: crate::value::shared_fields(fields),
            }))
        }
        "wraps" => {
            // wraps(wrapped) -> a decorator that stamps the wrapped function's
            // `__name__` onto the function it decorates. Modelled as a partial
            // over the internal `__apply_wraps__` builtin capturing the name:
            // `@wraps(f) def w(): ...` reduces to `w = __apply_wraps__(f_name, w)`,
            // which returns `w` with its `wraps_name` override set.
            let Some(wrapped) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "wraps() missing required argument".into(),
                )
                .into());
            };
            let wrapped_name = match wrapped {
                Value::Function(fd) => fd.wraps_name.clone().unwrap_or_else(|| fd.name.clone()),
                Value::Lambda(_) => "<lambda>".to_string(),
                other => other.type_name().to_string(),
            };
            Ok(Value::Partial(Box::new(crate::value::PartialData {
                func: Value::BuiltinName("__apply_wraps__".to_string()),
                args: vec![Value::String(wrapped_name.into())],
                keywords: indexmap::IndexMap::new(),
            })))
        }
        "reduce" => {
            // reduce(function, iterable[, initializer]) — fold left
            // over the iterable applying function(acc, item) at each
            // step. With no initializer, the first item seeds the
            // accumulator. With one, all items get folded into it.
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "reduce() requires a function argument".into(),
                )
                .into());
            }
            let func_val = args[0].clone();
            let iterable = args.get(1).ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "reduce() requires an iterable argument".into(),
                ))
            })?;
            let items = iterate_value(iterable)?;
            let initial = args.get(2).cloned();
            let mut iter = items.into_iter();
            let mut acc = match initial {
                Some(init) => init,
                None => match iter.next() {
                    Some(first) => first,
                    None => {
                        return Err(InterpreterError::TypeError(
                            "reduce() of empty sequence with no initial value".into(),
                        )
                        .into());
                    }
                },
            };
            // Route through the shared callable dispatcher so every
            // callable shape (BoundMethod, BuiltinTypeMethod,
            // ModuleFunction, sentinel strings, plus Function /
            // Lambda) works as the reducer -- same surface as
            // itertools' callbacks.
            for item in iter {
                let call_args = vec![acc, item];
                acc = crate::eval::modules::call_callable(
                    state,
                    &func_val,
                    &call_args,
                    &IndexMap::new(),
                    tools,
                )
                .await?;
            }
            Ok(acc)
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'functools' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// `functools` module registration. Genuinely async — `reduce(f, iter)`
/// re-enters the evaluator to call the user-supplied callable.
pub struct FunctoolsModule;

fn ensure_cmp_key_class(state: &mut InterpreterState) {
    if state.classes.contains_key(CMP_KEY_CLASS) {
        return;
    }
    state.classes.insert(CMP_KEY_CLASS.to_string(), ClassValue::new(CMP_KEY_CLASS));
}

/// Compare two `functools.CmpKey` instances via their stored cmp callable.
/// Returns `Some(result)` when both sides are CmpKey wrappers.
pub(crate) async fn try_cmp_key_lt(
    state: &mut InterpreterState,
    left: &Value,
    right: &Value,
    tools: &Tools,
) -> Option<Result<bool, EvalError>> {
    let (Value::Instance(a), Value::Instance(b)) = (left, right) else {
        return None;
    };
    if a.class_name != CMP_KEY_CLASS || b.class_name != CMP_KEY_CLASS {
        return None;
    }
    let (cmp, oa, ob) = {
        let af = a.fields.lock();
        let bf = b.fields.lock();
        (af.get("cmp")?.clone(), af.get("obj")?.clone(), bf.get("obj")?.clone())
    };
    // mycmp(a, b) -> negative / zero / positive
    Some(
        async {
            let result = crate::eval::functions::call_value_as_function(
                state,
                &cmp,
                &[oa, ob],
                &indexmap::IndexMap::new(),
                tools,
            )
            .await?;
            let n = match result {
                Value::Int(i) => i,
                Value::Bool(b) => i64::from(b),
                other => {
                    return Err(InterpreterError::TypeError(format!(
                        "cmp_to_key cmp must return int, got '{}'",
                        other.type_name()
                    ))
                    .into());
                }
            };
            Ok(n < 0)
        }
        .await,
    )
}

#[async_trait::async_trait]
impl crate::eval::modules::Module for FunctoolsModule {
    fn name(&self) -> &'static str {
        "functools"
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
        tools: &crate::tools::Tools,
    ) -> EvalResult {
        call(state, func, args, kwargs, tools).await
    }
}
