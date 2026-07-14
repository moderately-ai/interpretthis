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
//! - `copy.deepcopy` — fully independent clone of all nested shared storage,
//!   with a memo table so cyclic structures terminate.
//!
//! User `__copy__` / `__deepcopy__` hooks are not invoked (would require
//! async method dispatch from this sync module path). Documented in CONFORMANCE.

use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::{
    error::{EvalResult, InterpreterError},
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
            let mut memo = HashMap::new();
            Ok(deep_clone_memo(value, &mut memo))
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
        Value::List(items) => Value::List(items.clone()),
        Value::Instance(inst) => Value::Instance(inst.clone()),
        Value::Dict(map) => Value::Dict(map.clone()),
        Value::Set(items) => Value::Set(items.clone()),
        Value::Frozenset(items) => Value::Frozenset(items.clone()),
        Value::Tuple(items) => Value::Tuple(items.clone()),
        Value::Counter(map) => Value::Counter(map.clone()),
        Value::DefaultDict(data) => Value::DefaultDict(data.clone()),
        Value::Deque { items, maxlen } => Value::Deque { items: items.clone(), maxlen: *maxlen },
        other => other.clone(),
    }
}

/// Deep clone with cycle memo keyed by Arc identity for shared containers.
fn deep_clone_memo(value: &Value, memo: &mut HashMap<usize, Value>) -> Value {
    match value {
        Value::List(items) => {
            let key = Arc::as_ptr(items) as usize;
            if let Some(existing) = memo.get(&key) {
                return existing.clone();
            }
            let out = shared_list(Vec::new());
            memo.insert(key, Value::List(out.clone()));
            let guard = items.lock();
            let cloned: Vec<Value> = guard.iter().map(|v| deep_clone_memo(v, memo)).collect();
            *out.lock() = cloned;
            Value::List(out)
        }
        Value::Instance(inst) => {
            let key = Arc::as_ptr(&inst.fields) as usize;
            if let Some(existing) = memo.get(&key) {
                return existing.clone();
            }
            let out_fields = shared_fields(std::collections::BTreeMap::new());
            let out = Value::Instance(crate::value::InstanceValue {
                class_name: inst.class_name.clone(),
                fields: out_fields.clone(),
            });
            memo.insert(key, out.clone());
            let mut fields = std::collections::BTreeMap::new();
            for (k, v) in inst.fields.lock().iter() {
                fields.insert(k.clone(), deep_clone_memo(v, memo));
            }
            *out_fields.lock() = fields;
            out
        }
        Value::Tuple(items) => {
            Value::Tuple(items.iter().map(|v| deep_clone_memo(v, memo)).collect())
        }
        Value::Set(items) => Value::Set(items.iter().map(|v| deep_clone_memo(v, memo)).collect()),
        Value::Frozenset(items) => {
            Value::Frozenset(items.iter().map(|v| deep_clone_memo(v, memo)).collect())
        }
        Value::Dict(map) => {
            let mut out = IndexMap::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k.clone(), deep_clone_memo(v, memo));
            }
            Value::Dict(out)
        }
        Value::Counter(map) => {
            let mut out = IndexMap::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k.clone(), deep_clone_memo(v, memo));
            }
            Value::Counter(out)
        }
        Value::Deque { items, maxlen } => Value::Deque {
            items: items.iter().map(|v| deep_clone_memo(v, memo)).collect(),
            maxlen: *maxlen,
        },
        other => other.clone(),
    }
}

/// `copy` module registration.
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
        _state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &indexmap::IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        call(func, args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::shared_list;

    #[test]
    fn deep_clone_mutual_list_cycle() {
        let a = shared_list(Vec::new());
        let b = shared_list(Vec::new());
        a.lock().push(Value::List(b.clone()));
        b.lock().push(Value::List(a.clone()));
        let mut memo = HashMap::new();
        let c = deep_clone_memo(&Value::List(a), &mut memo);
        assert!(matches!(c, Value::List(_)));
        if let Value::List(c_arc) = c {
            assert_eq!(c_arc.lock().len(), 1);
            assert!(matches!(c_arc.lock()[0], Value::List(_)));
            if let Value::List(inner) = &c_arc.lock()[0] {
                assert_eq!(inner.lock().len(), 1);
            }
        }
    }
}
