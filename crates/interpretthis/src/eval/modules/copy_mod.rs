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
//! A user class's `__copy__` / `__deepcopy__` is dispatched when present — at
//! every level, so `deepcopy([obj])` runs `obj.__deepcopy__` too — which is why
//! the clone walkers are async (a Python method body runs via `call_method`).

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::{
    error::{EvalResult, InterpreterError},
    eval::{
        classes::{call_method, lookup_method_in_mro},
        functions::CallArgs,
    },
    state::InterpreterState,
    tools::Tools,
    value::{InstanceValue, Value, shared_dict, shared_fields, shared_list},
};

pub fn has_function(name: &str) -> bool {
    matches!(name, "copy" | "deepcopy")
}

pub async fn dispatch(
    state: &mut InterpreterState,
    func: &str,
    args: &[Value],
    tools: &Tools,
) -> EvalResult {
    match func {
        "copy" => {
            let Some(value) = args.first().cloned() else {
                return Err(InterpreterError::TypeError("copy() requires 1 argument".into()).into());
            };
            shallow_clone(state, &value, tools).await
        }
        "deepcopy" => {
            let Some(value) = args.first().cloned() else {
                return Err(
                    InterpreterError::TypeError("deepcopy() requires 1 argument".into()).into()
                );
            };
            let mut memo = HashMap::new();
            deep_clone_memo(state, &value, &mut memo, tools).await
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'copy' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// Shallow clone: the outer container gets a *fresh* backing store while its
/// elements stay shared. A user `__copy__` wins when defined.
async fn shallow_clone(state: &mut InterpreterState, value: &Value, tools: &Tools) -> EvalResult {
    if let Value::Instance(inst) = value {
        if let Some((_, method)) = lookup_method_in_mro(state, &inst.class_name, "__copy__") {
            let call = CallArgs { positional: &[], keyword: &IndexMap::new() };
            let (returned, _self) = call_method(state, &method, value.clone(), call, tools).await?;
            return Ok(returned);
        }
    }
    Ok(shallow_clone_structural(value))
}

/// The structural shallow clone (no `__copy__`): the Arc-backed variants (List,
/// Instance fields, ByteArray, Dict, Set) allocate a fresh store so mutations
/// don't alias the source; everything else keeps its elements shared.
fn shallow_clone_structural(value: &Value) -> Value {
    match value {
        Value::List(items) => Value::List(shared_list(items.lock().clone())),
        Value::Instance(inst) => Value::Instance(InstanceValue {
            class_name: inst.class_name.clone(),
            fields: shared_fields(inst.fields.lock().clone()),
        }),
        Value::ByteArray(bytes) => {
            Value::ByteArray(crate::value::shared_bytes(bytes.lock().clone()))
        }
        Value::Dict(map) => Value::Dict(shared_dict(map.lock().clone())),
        Value::OrderedDict(map) => Value::OrderedDict(shared_dict(map.lock().clone())),
        Value::Set(b) => Value::Set(crate::value::shared_set(b.lock().clone())),
        Value::Frozenset(items) => Value::Frozenset(items.clone()),
        Value::Tuple(items) => Value::Tuple(items.clone()),
        Value::Counter(map) => Value::Counter(map.clone()),
        Value::DefaultDict(data) => Value::DefaultDict(data.clone()),
        Value::Deque { items, maxlen } => Value::Deque { items: items.clone(), maxlen: *maxlen },
        other => other.clone(),
    }
}

/// Deep clone with a cycle memo keyed by Arc identity. A user `__deepcopy__` is
/// dispatched (at every level) when defined; otherwise the shared storage is
/// cloned independently. Async because `__deepcopy__` runs a Python method.
///
/// Every container snapshots under its lock and releases before the recursive
/// `.await` — a parking_lot guard cannot be held across an await, and a self
/// reference would otherwise re-lock the same mutex.
fn deep_clone_memo<'a>(
    state: &'a mut InterpreterState,
    value: &'a Value,
    memo: &'a mut HashMap<usize, Value>,
    tools: &'a Tools,
) -> Pin<Box<dyn Future<Output = EvalResult> + Send + 'a>> {
    Box::pin(async move {
        match value {
            Value::List(items) => {
                let key = Arc::as_ptr(items) as usize;
                if let Some(existing) = memo.get(&key) {
                    return Ok(existing.clone());
                }
                let out = shared_list(Vec::new());
                memo.insert(key, Value::List(out.clone()));
                let snapshot = items.lock().clone();
                let mut cloned = Vec::with_capacity(snapshot.len());
                for v in &snapshot {
                    cloned.push(deep_clone_memo(state, v, memo, tools).await?);
                }
                out.lock().set_items(cloned);
                Ok(Value::List(out))
            }
            Value::Instance(inst) => {
                let key = Arc::as_ptr(&inst.fields) as usize;
                if let Some(existing) = memo.get(&key) {
                    return Ok(existing.clone());
                }
                // A user `__deepcopy__(self, memo)` produces the whole copy. The
                // memo argument is an approximation (an empty dict) — enough for
                // the common `return Cls(...)` shape; sharing CPython's memo dict
                // across the user's own nested `copy.deepcopy` calls is not
                // modelled.
                if let Some((_, method)) =
                    lookup_method_in_mro(state, &inst.class_name, "__deepcopy__")
                {
                    let memo_arg = Value::Dict(shared_dict(IndexMap::new()));
                    let call = CallArgs {
                        positional: std::slice::from_ref(&memo_arg),
                        keyword: &IndexMap::new(),
                    };
                    let (returned, _self) =
                        call_method(state, &method, value.clone(), call, tools).await?;
                    memo.insert(key, returned.clone());
                    return Ok(returned);
                }
                let out_fields = shared_fields(BTreeMap::new());
                let out = Value::Instance(InstanceValue {
                    class_name: inst.class_name.clone(),
                    fields: out_fields.clone(),
                });
                memo.insert(key, out.clone());
                let snapshot = inst.fields.lock().clone();
                let mut fields = BTreeMap::new();
                for (k, v) in snapshot.iter() {
                    fields.insert(k.clone(), deep_clone_memo(state, v, memo, tools).await?);
                }
                *out_fields.lock() = fields;
                Ok(out)
            }
            Value::Dict(map) | Value::OrderedDict(map) => {
                let key = Arc::as_ptr(map) as usize;
                if let Some(existing) = memo.get(&key) {
                    return Ok(existing.clone());
                }
                let out = shared_dict(IndexMap::new());
                // Preserve the concrete type so a deep-copied OrderedDict stays
                // an OrderedDict.
                let wrap = |d| {
                    if matches!(value, Value::OrderedDict(_)) {
                        Value::OrderedDict(d)
                    } else {
                        Value::Dict(d)
                    }
                };
                memo.insert(key, wrap(out.clone()));
                let snapshot = map.lock().clone();
                let mut cloned = IndexMap::with_capacity(snapshot.len());
                for (k, v) in &snapshot {
                    cloned.insert(k.clone(), deep_clone_memo(state, v, memo, tools).await?);
                }
                out.lock().set_map(cloned);
                Ok(wrap(out))
            }
            Value::Tuple(items) => {
                let mut cloned = Vec::with_capacity(items.len());
                for v in items {
                    cloned.push(deep_clone_memo(state, v, memo, tools).await?);
                }
                Ok(Value::Tuple(cloned))
            }
            Value::Set(b) => {
                let items = b.lock().iter_ordered();
                let mut cloned = Vec::with_capacity(items.len());
                for v in &items {
                    cloned.push(deep_clone_memo(state, v, memo, tools).await?);
                }
                Ok(Value::new_set(cloned))
            }
            Value::Frozenset(b) => {
                let items = b.iter_ordered();
                let mut cloned = Vec::with_capacity(items.len());
                for v in &items {
                    cloned.push(deep_clone_memo(state, v, memo, tools).await?);
                }
                Ok(Value::new_frozenset(cloned))
            }
            Value::Counter(map) => {
                let mut cloned = IndexMap::with_capacity(map.len());
                for (k, v) in map {
                    cloned.insert(k.clone(), deep_clone_memo(state, v, memo, tools).await?);
                }
                Ok(Value::Counter(cloned))
            }
            Value::Deque { items, maxlen } => {
                let mut cloned = std::collections::VecDeque::with_capacity(items.len());
                for v in items {
                    cloned.push_back(deep_clone_memo(state, v, memo, tools).await?);
                }
                Ok(Value::Deque { items: cloned, maxlen: *maxlen })
            }
            // Fresh mutable-bytes store (bytes are scalars, no recursion).
            Value::ByteArray(bytes) => {
                Ok(Value::ByteArray(crate::value::shared_bytes(bytes.lock().clone())))
            }
            other => Ok(other.clone()),
        }
    })
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
        state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &indexmap::IndexMap<String, Value>,
        tools: &crate::tools::Tools,
    ) -> EvalResult {
        dispatch(state, func, args, tools).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InterpreterConfig;
    use crate::value::shared_list;

    #[tokio::test]
    async fn deep_clone_mutual_list_cycle() {
        let a = shared_list(Vec::new());
        let b = shared_list(Vec::new());
        a.lock().push(Value::List(b.clone()));
        b.lock().push(Value::List(a.clone()));
        let mut state = InterpreterState::new(InterpreterConfig::default());
        let tools = Tools::new();
        let mut memo = HashMap::new();
        let c = deep_clone_memo(&mut state, &Value::List(a), &mut memo, &tools).await.unwrap();
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
