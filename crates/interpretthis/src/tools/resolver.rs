// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tool resolution and dispatch.
//!
//! Given a call `name(args, **kwargs)`, decide whether `name` is registered as
//! a tool and, if so, dispatch to it — either eagerly (blocking the current
//! evaluator step) or as a spawned task that returns a `LazyProxy` handle.
//!
//! Before this module existed, the logic lived inline in `eval::functions::
//! eval_call`. Pulling it out keeps the call-routing switch simpler and lets
//! the dispatch path be tested in isolation.

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::{
    error::{EvalError, InterpreterError},
    eval::functions::resolve_proxy,
    state::InterpreterState,
    tools::{Tools, lazy_proxy::LazyProxy},
    value::Value,
};

/// Names that are builtins-rather-than-tools even if a host registers a tool
/// with the same identifier. Kept here (next to the dispatcher that enforces
/// it) so the priority is visible when auditing tool override behaviour.
const BUILTIN_NAMES: &[&str] = &[
    "print",
    "len",
    "range",
    "str",
    "int",
    "float",
    "complex",
    "bool",
    "type",
    "isinstance",
    "issubclass",
    "super",
    "hasattr",
    "callable",
    "abs",
    "round",
    "min",
    "max",
    "sum",
    "all",
    "any",
    "sorted",
    "enumerate",
    "zip",
    "reversed",
    "chr",
    "ord",
    "list",
    "tuple",
    "dict",
    "set",
    "iter",
    "next",
    "filter",
    "map",
    "repr",
    "hash",
    "id",
    "input",
    "object",
];

/// True if `name` is a Python builtin the interpreter handles natively.
/// Builtins win over identically-named host tools, so the dispatcher
/// short-circuits without hitting the tool registry.
#[must_use]
pub fn is_builtin_name(name: &str) -> bool {
    BUILTIN_NAMES.contains(&name)
}

/// Per-call descriptor for [`resolve_and_dispatch`].
///
/// Bundles the name + positional + keyword args so callers see the
/// three pieces of payload as a unit, separate from the
/// `&InterpreterState` and `&Tools` deps the dispatcher reads.
pub struct ToolCallDescriptor<'a> {
    /// Identifier the Python source called (e.g. `f(...)` → `"f"`).
    pub name: &'a str,
    /// Positional arguments (already evaluated to `Value`s).
    pub args: &'a [Value],
    /// Keyword arguments (already evaluated to `Value`s).
    pub kwargs: &'a IndexMap<String, Value>,
}

/// Attempt to resolve `name` against the registered tool set and dispatch if
/// found.
///
/// Returns:
/// - `Ok(Some(value))` if a tool matched and ran (eagerly) or was spawned (returning a `LazyProxy`
///   handle).
/// - `Ok(None)` if `name` is not a tool (the caller should continue with builtins, user-defined
///   functions, etc.).
/// - `Err(_)` on tool execution errors or argument-resolution failures.
pub async fn resolve_and_dispatch(
    state: &InterpreterState,
    call: ToolCallDescriptor<'_>,
    tools: &Tools,
) -> Result<Option<Value>, EvalError> {
    let ToolCallDescriptor { name, args, kwargs } = call;

    if is_builtin_name(name) {
        return Ok(None);
    }

    let Some(tool_config) = tools.get(name) else { return Ok(None) };

    // Resolve any LazyProxy arguments to their final values before handing
    // them to the tool — creates the dependency chain the scheduler relies on.
    let mut resolved_args: Vec<Value> = Vec::with_capacity(args.len());
    for arg in args {
        resolved_args.push(resolve_proxy(arg).await?);
    }
    let mut resolved_kwargs: HashMap<String, Value> = HashMap::new();
    for (k, v) in kwargs {
        resolved_kwargs.insert(k.clone(), resolve_proxy(v).await?);
    }

    // Normalise call shape: positionals become `arg{i}`, kwargs merge over top.
    // Keeps the handler surface uniform whether the caller writes
    // `f(x)` or `f(x=...)`.
    let mut tool_kwargs: HashMap<String, Value> = HashMap::new();
    for (i, arg) in resolved_args.iter().enumerate() {
        tool_kwargs.insert(format!("arg{i}"), arg.clone());
    }
    for (k, v) in &resolved_kwargs {
        tool_kwargs.insert(k.clone(), v.clone());
    }

    let tool_timeout = remaining_tool_timeout(state);

    if tool_config.parallelizable {
        // Spawn as a background task — the caller gets a LazyProxy handle
        // that materialises when awaited. The semaphore bounds in-flight
        // concurrency. When max_execution_time is set, the task itself is
        // capped to the remaining budget so a hung tool cannot outlive it.
        let semaphore = state.tool_semaphore.clone();
        let handler = tool_config.handler.clone();
        let tool_name = name.to_string();

        let handle = tokio::spawn(async move {
            let _permit = semaphore.acquire_owned().await;
            call_handler(handler.as_ref(), tool_kwargs, tool_timeout).await
        });

        return Ok(Some(Value::LazyProxy(LazyProxy::new(handle, tool_name))));
    }

    match call_handler(tool_config.handler.as_ref(), tool_kwargs, tool_timeout).await {
        Ok(val) => Ok(Some(val)),
        Err(e) => {
            Err(InterpreterError::Tool { tool_name: name.to_string(), message: e.message }.into())
        }
    }
}

/// Remaining wall-clock budget for a tool call, if configured.
fn remaining_tool_timeout(state: &InterpreterState) -> Option<std::time::Duration> {
    let max = state.config.max_execution_time?;
    Some(max.saturating_sub(state.execution_start.elapsed()))
}

async fn call_handler(
    handler: &dyn crate::tools::ToolHandler,
    tool_kwargs: HashMap<String, Value>,
    timeout: Option<std::time::Duration>,
) -> Result<Value, crate::tools::ToolError> {
    use crate::tools::ToolError;
    match timeout {
        Some(d) if d.is_zero() => {
            Err(ToolError::new("tool timed out (no remaining execution budget)"))
        }
        Some(d) => match tokio::time::timeout(d, handler.call(tool_kwargs)).await {
            Ok(r) => r,
            Err(_) => Err(ToolError::new(format!(
                "tool timed out after {d:?} (max_execution_time budget)"
            ))),
        },
        None => handler.call(tool_kwargs).await,
    }
}
