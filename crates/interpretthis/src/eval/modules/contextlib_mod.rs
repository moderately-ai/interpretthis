// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of a subset of Python's `contextlib` module.
//!
//! - [`nullcontext`](https://docs.python.org/3/library/contextlib.html#contextlib.nullcontext)
//! - [`suppress`](https://docs.python.org/3/library/contextlib.html#contextlib.suppress)
//!
//! `@contextmanager` is **not** implemented yet (tracked by
//! `gap-contextlib-contextmanager-decorator`): it requires suspending a
//! generator between `__enter__` and `__exit__`, which our eager-yield
//! model cannot do. User classes with `__enter__`/`__exit__` remain the
//! supported path for custom context managers.

use indexmap::IndexMap;

use crate::{
    error::{EvalResult, InterpreterError},
    eval::exceptions::matches_user_exception,
    state::InterpreterState,
    tools::Tools,
    value::{ExceptionValue, InstanceValue, Value},
};

/// Class names used as markers for `call_context_method` special-cases.
pub const NULLCONTEXT_CLASS: &str = "contextlib.nullcontext";
pub const SUPPRESS_CLASS: &str = "contextlib.suppress";

pub fn has_function(name: &str) -> bool {
    matches!(name, "nullcontext" | "suppress" | "contextmanager")
}

/// `contextlib` module registration.
pub struct ContextlibModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for ContextlibModule {
    fn name(&self) -> &'static str {
        "contextlib"
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        state: &mut InterpreterState,
        func: &str,
        args: &[Value],
        kwargs: &IndexMap<String, Value>,
        tools: &Tools,
    ) -> EvalResult {
        call(state, func, args, kwargs, tools).await
    }
}

async fn call(
    state: &mut InterpreterState,
    func: &str,
    args: &[Value],
    _kwargs: &IndexMap<String, Value>,
    _tools: &Tools,
) -> EvalResult {
    ensure_marker_classes(state);
    match func {
        "nullcontext" => {
            let enter_result = args.first().cloned().unwrap_or(Value::None);
            let mut fields = std::collections::BTreeMap::new();
            fields.insert("enter_result".into(), enter_result);
            Ok(Value::Instance(InstanceValue {
                class_name: NULLCONTEXT_CLASS.into(),
                fields: crate::value::shared_fields(fields),
            }))
        }
        "suppress" => {
            // Store exception type names (or ExceptionType values) as a list.
            let mut names: Vec<Value> = Vec::new();
            for a in args {
                match a {
                    Value::ExceptionType(n) | Value::Class(n) => {
                        names.push(Value::String(n.clone().into()));
                    }
                    Value::String(s) => names.push(Value::String(s.clone())),
                    other => {
                        return Err(InterpreterError::TypeError(format!(
                            "suppress() arguments must be exception types, not '{}'",
                            other.type_name()
                        ))
                        .into());
                    }
                }
            }
            let mut fields = std::collections::BTreeMap::new();
            fields.insert("exceptions".into(), Value::List(crate::value::shared_list(names)));
            Ok(Value::Instance(InstanceValue {
                class_name: SUPPRESS_CLASS.into(),
                fields: crate::value::shared_fields(fields),
            }))
        }
        "contextmanager" => Err(InterpreterError::TypeError(
            "@contextmanager is not supported (requires suspended generators; \
             see CONFORMANCE.md#unsupported-language-features); use a class with \
             __enter__/__exit__ instead"
                .into(),
        )
        .into()),
        other => Err(InterpreterError::AttributeError(format!(
            "module 'contextlib' has no attribute '{other}'"
        ))
        .into()),
    }
}

/// Register empty marker classes so Instance lookups don't NameError.
fn ensure_marker_classes(state: &mut InterpreterState) {
    use crate::value::ClassValue;
    for name in [NULLCONTEXT_CLASS, SUPPRESS_CLASS] {
        if state.classes.contains_key(name) {
            continue;
        }
        state.classes.insert(name.to_string(), ClassValue::new(name));
    }
}

/// Handle `__enter__` / `__exit__` for contextlib marker instances.
/// Returns `None` when `receiver` is not a contextlib helper.
pub(crate) fn try_contextlib_method(
    state: &InterpreterState,
    receiver: &Value,
    method: &str,
    args: &[Value],
) -> Option<EvalResult> {
    let Value::Instance(inst) = receiver else {
        return None;
    };
    match (inst.class_name.as_str(), method) {
        (NULLCONTEXT_CLASS, "__enter__") => {
            Some(Ok(inst.fields.lock().get("enter_result").cloned().unwrap_or(Value::None)))
        }
        (NULLCONTEXT_CLASS, "__exit__") => Some(Ok(Value::Bool(false))),
        (SUPPRESS_CLASS, "__enter__") => Some(Ok(Value::None)),
        (SUPPRESS_CLASS, "__exit__") => Some(suppress_exit(state, inst, args)),
        _ => None,
    }
}

fn suppress_exit(state: &InterpreterState, inst: &InstanceValue, args: &[Value]) -> EvalResult {
    // __exit__(exc_type, exc_val, tb) — suppress when exc matches listed types.
    let exc_type = args.first();
    let exc_val = args.get(1);
    if matches!(exc_type, None | Some(Value::None)) {
        return Ok(Value::Bool(false));
    }
    let names: Vec<String> = {
        let fields = inst.fields.lock();
        let Some(Value::List(list)) = fields.get("exceptions") else {
            return Ok(Value::Bool(false));
        };
        list.lock()
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.to_string()),
                _ => None,
            })
            .collect()
    };
    if names.is_empty() {
        return Ok(Value::Bool(false));
    }

    // Build a synthetic ExceptionValue for matching.
    let type_name = match exc_type {
        Some(Value::ExceptionType(n)) | Some(Value::Class(n)) => n.clone(),
        Some(Value::String(n)) => n.to_string(),
        Some(Value::Exception(e)) => e.type_name.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    let message = match exc_val {
        Some(Value::Exception(e)) => e.message.clone(),
        Some(v) => format!("{v}"),
        None => String::new(),
    };
    let exc = ExceptionValue::new(type_name, message);

    for name in &names {
        if name == "Exception" || name == "BaseException" {
            return Ok(Value::Bool(true));
        }
        if exc.type_name == *name {
            return Ok(Value::Bool(true));
        }
        // Reuse hierarchy helpers via a thin Value::ExceptionType probe.
        let probe = Value::ExceptionType(name.clone());
        // Inline the same rules as matches_exception_type without importing private fns.
        if crate::eval::functions::is_exception_type_name(name)
            && exception_name_matches(&exc.type_name, name)
        {
            return Ok(Value::Bool(true));
        }
        if matches_user_exception(state, &exc, name) {
            return Ok(Value::Bool(true));
        }
        let _ = probe;
    }
    Ok(Value::Bool(false))
}

fn exception_name_matches(exc_name: &str, parent: &str) -> bool {
    // Mirror exceptions::builtin_exception_issubclass (kept local to avoid
    // pub(crate) churn on a private helper).
    let mut cur = exc_name;
    for _ in 0..16 {
        if cur == parent {
            return true;
        }
        cur = match cur {
            "ZeroDivisionError" | "OverflowError" | "FloatingPointError" => "ArithmeticError",
            "KeyError" | "IndexError" => "LookupError",
            "FileNotFoundError" | "PermissionError" | "TimeoutError" | "IOError" => "OSError",
            "NotImplementedError" | "RecursionError" => "RuntimeError",
            "AssertionError" | "AttributeError" | "NameError" | "TypeError" | "ValueError"
            | "RuntimeError" | "OSError" | "LookupError" | "ArithmeticError" | "StopIteration" => {
                "Exception"
            }
            "Exception" => "BaseException",
            _ => return false,
        };
    }
    false
}
