// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `copy` module.
//!
//! Documented divergence: `Value` in this interpreter is fully owned —
//! every clone deep-copies inner collections automatically. CPython's
//! `copy.copy` (shallow) preserves reference identity for inner
//! mutable members, so a `shallow[0].append(x)` mutates the original.
//! Our model has no reference identity, so `copy.copy` and
//! `copy.deepcopy` both produce an independent clone — semantics
//! matching CPython's `deepcopy` behaviour. Code that relies on the
//! shallow-share distinction will see a divergence, but the more
//! common pattern (just "give me a clone I can mutate safely") works
//! identically.

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    value::{Value, shared_list},
};

pub fn has_function(name: &str) -> bool {
    matches!(name, "copy" | "deepcopy")
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        "copy" | "deepcopy" => {
            let Some(value) = args.first() else {
                return Err(
                    InterpreterError::TypeError(format!("{func}() requires 1 argument")).into()
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

/// Produce an independent clone of `value`. For shared-storage variants
/// (`Value::List`) this allocates a fresh `SharedList`; for nested
/// collections (`Dict`, `Set`, `Tuple`, `Instance`) it recurses so the
/// returned value shares no mutable storage with the input. Matches the
/// documented "owned-clone" semantics referenced at the top of this
/// module — without this we'd inherit the D2 Arc-share, which would
/// make every `copy.copy` return an alias.
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
        Value::Instance(inst) => {
            let mut fields = std::collections::BTreeMap::new();
            for (k, v) in &inst.fields {
                fields.insert(k.clone(), deep_clone(v));
            }
            Value::Instance(crate::value::InstanceValue {
                class_name: inst.class_name.clone(),
                fields,
            })
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
