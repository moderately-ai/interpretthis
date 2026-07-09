// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `copy` module.
//!
//! With shared-storage variants ([`SharedList`], [`SharedFields`]), we can
//! honour CPython's shallow vs deep distinction:
//!
//! - `copy.copy` — clone the outer container; nested lists/instances keep
//!   the same Arc (mutations through either alias are shared).
//! - `copy.deepcopy` — fully independent clone of all nested shared storage.

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    value::{Value, shared_fields, shared_list},
};

pub fn has_function(name: &str) -> bool {
    matches!(name, "copy" | "deepcopy")
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        "copy" => {
            let Some(value) = args.first() else {
                return Err(InterpreterError::TypeError("copy() requires 1 argument".into()).into());
            };
            Ok(shallow_clone(value))
        }
        "deepcopy" => {
            let Some(value) = args.first() else {
                return Err(
                    InterpreterError::TypeError("deepcopy() requires 1 argument".into()).into()
                );
            };
            Ok(deep_clone(value))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'copy' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// Shallow clone: outer container is new; nested shared storage is shared.
fn shallow_clone(value: &Value) -> Value {
    match value {
        // List / Instance: Arc clone = shared identity for inners.
        Value::List(items) => Value::List(items.clone()),
        Value::Instance(inst) => Value::Instance(inst.clone()),
        // Dict / set / tuple: new outer; Values inside are cloned shallowly
        // (nested lists still share Arc).
        Value::Dict(map) => Value::Dict(map.clone()),
        Value::Set(items) => Value::Set(items.clone()),
        Value::Tuple(items) => Value::Tuple(items.clone()),
        Value::Counter(map) => Value::Counter(map.clone()),
        Value::DefaultDict(data) => Value::DefaultDict(data.clone()),
        Value::Deque { items, maxlen } => Value::Deque { items: items.clone(), maxlen: *maxlen },
        other => other.clone(),
    }
}

/// Fully independent clone of all nested shared storage.
fn deep_clone(value: &Value) -> Value {
    match value {
        Value::List(items) => {
            let guard = items.lock();
            Value::List(shared_list(guard.iter().map(deep_clone).collect()))
        }
        Value::Tuple(items) => Value::Tuple(items.iter().map(deep_clone).collect()),
        Value::Set(items) => Value::Set(items.iter().map(deep_clone).collect()),
        Value::Dict(map) => {
            let mut out = IndexMap::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k.clone(), deep_clone(v));
            }
            Value::Dict(out)
        }
        Value::Counter(map) => {
            let mut out = IndexMap::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k.clone(), deep_clone(v));
            }
            Value::Counter(out)
        }
        Value::Instance(inst) => {
            let mut fields = std::collections::BTreeMap::new();
            for (k, v) in inst.fields.lock().iter() {
                fields.insert(k.clone(), deep_clone(v));
            }
            Value::Instance(crate::value::InstanceValue {
                class_name: inst.class_name.clone(),
                fields: shared_fields(fields),
            })
        }
        Value::Deque { items, maxlen } => {
            Value::Deque { items: items.iter().map(deep_clone).collect(), maxlen: *maxlen }
        }
        other => other.clone(),
    }
}

pub struct CopyModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for CopyModule {
    fn name(&self) -> &'static str {
        "copy"
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        _state: &mut InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> Result<Value, EvalError> {
        call(func, args)
    }
}
