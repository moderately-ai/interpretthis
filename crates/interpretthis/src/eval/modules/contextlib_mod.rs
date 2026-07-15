// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of a subset of Python's `contextlib` module.
//!
//! - [`nullcontext`](https://docs.python.org/3/library/contextlib.html#contextlib.nullcontext)
//! - [`suppress`](https://docs.python.org/3/library/contextlib.html#contextlib.suppress)
//! - [`contextmanager`](https://docs.python.org/3/library/contextlib.html#contextlib.contextmanager)
//!   — wraps a generator into a context manager that suspends across
//!   `__enter__`/`__exit__` (the generator is stepped through the
//!   suspend engine; a single `yield` inside `try`/`finally` runs its
//!   teardown on exit, not on suspend).

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::exceptions::matches_user_exception,
    state::InterpreterState,
    tools::Tools,
    value::{ExceptionValue, InstanceValue, Value},
};

/// Class names used as markers for `call_context_method` special-cases.
pub const NULLCONTEXT_CLASS: &str = "contextlib.nullcontext";
pub const SUPPRESS_CLASS: &str = "contextlib.suppress";
/// Marker class for a `@contextmanager`-produced context manager; the
/// wrapped generator lives in its `gen` field.
pub const GENCM_CLASS: &str = "contextlib._GeneratorContextManager";
/// Marker for `contextlib.redirect_stdout(target)`; its `target` field holds
/// the StringIO that receives `print` output while the block is active.
pub const REDIRECT_STDOUT_CLASS: &str = "contextlib.redirect_stdout";
/// Marker for `contextlib.ExitStack`; its `registered` field is a list of
/// deferred cleanups (context managers and callbacks) unwound on exit.
pub const EXITSTACK_CLASS: &str = "contextlib.ExitStack";

pub fn has_function(name: &str) -> bool {
    matches!(name, "nullcontext" | "suppress" | "contextmanager" | "redirect_stdout" | "ExitStack")
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
        // `@contextmanager` wraps a generator function. Calling the
        // wrapped function must produce a fresh context manager, so
        // return a Partial over the internal `__gen_contextmanager__`
        // builtin bound to the decorated generator function; invoking it
        // runs the generator and boxes it in a `_GeneratorContextManager`
        // instance.
        "contextmanager" => {
            let func = args.first().cloned().unwrap_or(Value::None);
            Ok(Value::Partial(Box::new(crate::value::PartialData {
                func: Value::BuiltinName("__gen_contextmanager__".into()),
                args: vec![func],
                keywords: IndexMap::new(),
            })))
        }
        // `redirect_stdout(target)` — the marker stores the target stream; the
        // push/pop of the redirect stack happens in `call_context_method`
        // (which owns `&mut state`).
        "redirect_stdout" => {
            let target = args.first().cloned().unwrap_or(Value::None);
            if !matches!(target, Value::StringIO(_)) {
                return Err(InterpreterError::TypeError(
                    "redirect_stdout() requires an io.StringIO target".into(),
                )
                .into());
            }
            let mut fields = std::collections::BTreeMap::new();
            fields.insert("target".into(), target);
            Ok(Value::Instance(InstanceValue {
                class_name: REDIRECT_STDOUT_CLASS.into(),
                fields: crate::value::shared_fields(fields),
            }))
        }
        "ExitStack" => {
            let mut fields = std::collections::BTreeMap::new();
            fields.insert("registered".into(), Value::List(crate::value::shared_list(Vec::new())));
            Ok(Value::Instance(InstanceValue {
                class_name: EXITSTACK_CLASS.into(),
                fields: crate::value::shared_fields(fields),
            }))
        }
        other => Err(InterpreterError::AttributeError(format!(
            "module 'contextlib' has no attribute '{other}'"
        ))
        .into()),
    }
}

/// Box a running generator (from a `@contextmanager` function) into a
/// `_GeneratorContextManager` instance for the `with` machinery.
pub fn wrap_generator_cm(state: &mut InterpreterState, generator: Value) -> Value {
    ensure_marker_classes(state);
    let mut fields = std::collections::BTreeMap::new();
    fields.insert("gen".into(), generator);
    Value::Instance(InstanceValue {
        class_name: GENCM_CLASS.into(),
        fields: crate::value::shared_fields(fields),
    })
}

/// Async `__enter__` / `__exit__` for a `@contextmanager` instance —
/// steps the wrapped generator (which `try_contextlib_method` can't do,
/// being sync). Returns `None` when `receiver` isn't a generator CM.
pub(crate) async fn try_gencm_method(
    state: &mut InterpreterState,
    receiver: &Value,
    method: &str,
    args: &[Value],
    tools: &Tools,
) -> Option<EvalResult> {
    let Value::Instance(inst) = receiver else { return None };
    if inst.class_name != GENCM_CLASS {
        return None;
    }
    let generator = inst.fields.lock().get("gen").cloned()?;
    let empty = IndexMap::new();
    match method {
        "__enter__" => {
            let stepped = crate::eval::functions::dispatch_generator_method(
                state,
                &generator,
                "__next__",
                &[],
                &empty,
                tools,
            )
            .await;
            Some(match stepped {
                Ok(v) => Ok(v),
                Err(EvalError::Exception(e)) if e.type_name == "StopIteration" => {
                    Err(InterpreterError::Runtime("generator didn't yield".into()).into())
                }
                Err(e) => Err(e),
            })
        }
        "__exit__" => {
            // args = (exc_type, exc_val, traceback).
            let has_exc = !matches!(args.first(), None | Some(Value::None));
            if has_exc {
                // Exception exit: throw it in. Swallowed (returns →
                // StopIteration) suppresses; a re-raise / new raise
                // propagates; yielding again is an error.
                let exc = args
                    .get(1)
                    .filter(|v| !matches!(v, Value::None))
                    .or_else(|| args.first())
                    .cloned()
                    .unwrap_or(Value::None);
                let thrown = crate::eval::functions::dispatch_generator_method(
                    state,
                    &generator,
                    "throw",
                    std::slice::from_ref(&exc),
                    &empty,
                    tools,
                )
                .await;
                return Some(match thrown {
                    Ok(_) => {
                        Err(InterpreterError::Runtime("generator didn't stop after throw".into())
                            .into())
                    }
                    Err(EvalError::Exception(e)) if e.type_name == "StopIteration" => {
                        Ok(Value::Bool(true))
                    }
                    Err(e) => Err(e),
                });
            }
            // Normal exit: resume; it must stop (StopIteration).
            let stepped = crate::eval::functions::dispatch_generator_method(
                state,
                &generator,
                "__next__",
                &[],
                &empty,
                tools,
            )
            .await;
            Some(match stepped {
                Ok(_) => Err(InterpreterError::Runtime("generator didn't stop".into()).into()),
                Err(EvalError::Exception(e)) if e.type_name == "StopIteration" => {
                    Ok(Value::Bool(false))
                }
                Err(e) => Err(e),
            })
        }
        _ => None,
    }
}

/// `contextlib.ExitStack` methods. Handles the method calls made inside the
/// block (`enter_context`, `callback`, `close`) as well as the `with`
/// protocol (`__enter__`/`__exit__`), unwinding registered cleanups in LIFO
/// order. Returns `None` when `receiver` is not an ExitStack.
pub(crate) async fn try_exitstack_method(
    state: &mut InterpreterState,
    receiver: &Value,
    method: &str,
    args: &[Value],
    tools: &Tools,
) -> Option<EvalResult> {
    let Value::Instance(inst) = receiver else { return None };
    if inst.class_name != EXITSTACK_CLASS {
        return None;
    }
    // The `registered` field is a shared list of cleanups, each a
    // `("cm", manager)` or `("cb", func, (args…))` tuple.
    let registered = match inst.fields.lock().get("registered").cloned() {
        Some(Value::List(l)) => l,
        _ => return Some(Err(InterpreterError::Runtime("ExitStack state lost".into()).into())),
    };
    match method {
        "__enter__" => Some(Ok(receiver.clone())),
        "enter_context" => {
            let cm = args.first().cloned().unwrap_or(Value::None);
            let enter_result = match Box::pin(crate::eval::control_flow::call_context_method(
                state,
                &cm,
                "__enter__",
                &[],
                tools,
            ))
            .await
            {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
            registered.lock().push(Value::Tuple(vec![Value::String("cm".into()), cm]));
            Some(Ok(enter_result))
        }
        "callback" => {
            let func = args.first().cloned().unwrap_or(Value::None);
            let cb_args = Value::Tuple(args.get(1..).unwrap_or(&[]).to_vec());
            registered.lock().push(Value::Tuple(vec![
                Value::String("cb".into()),
                func.clone(),
                cb_args,
            ]));
            // ExitStack.callback returns the callback so it can be used as a
            // decorator.
            Some(Ok(func))
        }
        "close" | "__exit__" => {
            // Unwind in reverse; on `__exit__`, args = (exc_type, exc_val, tb).
            let exit_args: [Value; 3] = [
                args.first().cloned().unwrap_or(Value::None),
                args.get(1).cloned().unwrap_or(Value::None),
                args.get(2).cloned().unwrap_or(Value::None),
            ];
            let items: Vec<Value> = {
                let mut guard = registered.lock();
                let out = guard.clone();
                guard.clear();
                out
            };
            let mut suppressed = false;
            for item in items.into_iter().rev() {
                let Value::Tuple(parts) = item else { continue };
                match parts.first() {
                    Some(Value::String(tag)) if tag.as_str() == "cm" => {
                        let cm = parts.get(1).cloned().unwrap_or(Value::None);
                        match Box::pin(crate::eval::control_flow::call_context_method(
                            state, &cm, "__exit__", &exit_args, tools,
                        ))
                        .await
                        {
                            Ok(v) if v.is_truthy() => suppressed = true,
                            Ok(_) => {}
                            Err(e) => return Some(Err(e)),
                        }
                    }
                    Some(Value::String(tag)) if tag.as_str() == "cb" => {
                        let func = parts.get(1).cloned().unwrap_or(Value::None);
                        let cb_args = match parts.get(2) {
                            Some(Value::Tuple(a)) => a.clone(),
                            _ => Vec::new(),
                        };
                        let empty = IndexMap::new();
                        if let Err(e) = crate::eval::functions::call_value_as_function(
                            state, &func, &cb_args, &empty, tools,
                        )
                        .await
                        {
                            return Some(Err(e));
                        }
                    }
                    _ => {}
                }
            }
            if method == "close" {
                Some(Ok(Value::None))
            } else {
                Some(Ok(Value::Bool(suppressed)))
            }
        }
        _ => None,
    }
}

/// Register empty marker classes so Instance lookups don't NameError.
fn ensure_marker_classes(state: &mut InterpreterState) {
    use crate::value::ClassValue;
    for name in
        [NULLCONTEXT_CLASS, SUPPRESS_CLASS, GENCM_CLASS, REDIRECT_STDOUT_CLASS, EXITSTACK_CLASS]
    {
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
