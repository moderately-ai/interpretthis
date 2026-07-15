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
    value::{Value, shared_dict, shared_fields, shared_list},
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

/// Shallow clone: the outer container gets a *fresh* backing store while
/// its elements stay shared (nested Arc-backed values keep their Arc).
/// The Arc-backed variants (List, Instance fields, ByteArray) must
/// allocate a new store — `Arc::clone` would alias the original and make
/// `copy.copy(x)` mutations visible through `x`.
fn shallow_clone(value: &Value) -> Value {
    match value {
        Value::List(items) => Value::List(shared_list(items.lock().clone())),
        Value::Instance(inst) => Value::Instance(crate::value::InstanceValue {
            class_name: inst.class_name.clone(),
            fields: shared_fields(inst.fields.lock().clone()),
        }),
        Value::ByteArray(bytes) => {
            Value::ByteArray(crate::value::shared_bytes(bytes.lock().clone()))
        }
        // Dict is Arc-shared, so a fresh backing store is needed (an
        // Arc clone would alias the source); its elements stay shared.
        Value::Dict(map) => Value::Dict(shared_dict(map.lock().clone())),
        // A shallow copy is a *distinct* mutable set with the same elements, so
        // clone the body into a fresh handle (an Arc bump would alias mutation).
        // A frozenset is immutable, so sharing the Arc is fine.
        Value::Set(b) => Value::Set(crate::value::shared_set(b.lock().clone())),
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
        Value::Set(b) => Value::new_set(
            b.lock().iter_ordered().iter().map(|v| deep_clone_memo(v, memo)).collect(),
        ),
        Value::Frozenset(b) => Value::new_frozenset(
            b.iter_ordered().iter().map(|v| deep_clone_memo(v, memo)).collect(),
        ),
        Value::Dict(map) => {
            let snapshot = map.lock().clone();
            let mut out = IndexMap::with_capacity(snapshot.len());
            for (k, v) in &snapshot {
                out.insert(k.clone(), deep_clone_memo(v, memo));
            }
            Value::Dict(crate::value::shared_dict(out))
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
        // Fresh mutable-bytes store (bytes are scalars, no recursion).
        Value::ByteArray(bytes) => {
            Value::ByteArray(crate::value::shared_bytes(bytes.lock().clone()))
        }
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
