// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-call parameter binding and body execution, plus the def-time
//! default-argument evaluation helper.
//!
//! `bind_params` maps positional + keyword args onto a function's
//! `FunctionParams` spec, preferring def-time-evaluated default
//! values and falling back to a source-string re-eval only on
//! state imports that predate the def-time landing.
//!
//! `evaluate_param_defaults` walks the default expression sources at
//! `def` time and stashes the values on the `FunctionParams` —
//! CPython semantics. Called from `eval_function_def`,
//! `eval_lambda_def`, and `eval::classes::eval_class_def`'s method
//! loop.
//!
//! These pieces live in their own module so the call-site files
//! (`functions/mod.rs`) and `eval::classes::call_method` can all
//! consume them without inflating `functions/mod.rs`.

use indexmap::IndexMap;
use rustc_hash::FxHashMap;
use rustpython_parser::ast;

use crate::{
    error::{ControlFlow, EvalError, EvalResult, InterpreterError},
    eval::{eval_expr, eval_stmt},
    state::InterpreterState,
    tools::Tools,
    value::{FunctionParams, Value, ValueKey},
};

/// Parse and evaluate a single default-argument source string. Used
/// at function/lambda definition time to capture the values once
/// (CPython semantics), and as a fallback inside [`bind_params`] when
/// `default_values` is empty on imported state.
async fn eval_default_source(
    state: &mut InterpreterState,
    tools: &Tools,
    source: &str,
) -> Result<Value, EvalError> {
    let stmts = crate::parser::parse(source).map_err(|e| {
        EvalError::from(InterpreterError::Runtime(format!(
            "failed to parse default expression '{source}': {e}"
        )))
    })?;
    let first = stmts.into_iter().next().ok_or_else(|| {
        EvalError::from(InterpreterError::Runtime(format!(
            "default expression '{source}' produced no statements"
        )))
    })?;
    let ast::Stmt::Expr(expr_stmt) = first else {
        return Err(InterpreterError::Runtime(format!(
            "default expression '{source}' did not parse as a bare expression"
        ))
        .into());
    };
    eval_expr(state, &expr_stmt.value, tools).await
}

/// Evaluate every default expression on `params` against the current
/// state and stash the values on `params.default_values` /
/// `params.kw_default_values`. Called at function/lambda definition
/// time so the same default value object is reused across calls —
/// matching CPython.
///
/// Idempotent and resilient: if `default_values` is already
/// populated (e.g. on imported state from a future-format blob), it's
/// left untouched.
pub(crate) async fn evaluate_param_defaults(
    state: &mut InterpreterState,
    params: &mut FunctionParams,
    tools: &Tools,
) -> Result<(), EvalError> {
    if params.default_values.is_empty() && !params.defaults.is_empty() {
        let mut values = Vec::with_capacity(params.defaults.len());
        for src in &params.defaults {
            values.push(eval_default_source(state, tools, src).await?);
        }
        params.default_values = values;
    }
    if params.kw_default_values.is_empty() && !params.kw_defaults.is_empty() {
        let mut values = Vec::with_capacity(params.kw_defaults.len());
        for opt_src in &params.kw_defaults {
            let v = match opt_src {
                Some(src) => Some(eval_default_source(state, tools, src).await?),
                None => None,
            };
            values.push(v);
        }
        params.kw_default_values = values;
    }
    Ok(())
}

/// Bind function arguments to parameter names.
pub(crate) async fn bind_params(
    params: &FunctionParams,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    state: &mut InterpreterState,
    tools: &Tools,
) -> Result<FxHashMap<String, Value>, EvalError> {
    let capacity = params.args.len()
        + usize::from(params.vararg.is_some())
        + params.kwonlyargs.len()
        + usize::from(params.kwarg.is_some());
    let mut scope = FxHashMap::with_capacity_and_hasher(capacity, Default::default());
    let num_params = params.args.len();
    let num_defaults = params.defaults.len();
    let first_default = num_params.saturating_sub(num_defaults);

    // Bind positional arguments. Defaults prefer the def-time
    // evaluated values; the source-string re-eval path is the
    // legacy fallback for state imported before def-time evaluation
    // landed (`default_values` will be empty in that case).
    for (i, param) in params.args.iter().enumerate() {
        if i < args.len() {
            scope.insert(param.name.clone(), args[i].clone());
        } else if let Some(val) = kwargs.get(&param.name) {
            scope.insert(param.name.clone(), val.clone());
        } else {
            // Check defaults
            let default_idx = i.checked_sub(first_default);
            if let Some(di) = default_idx {
                let default_val = if di < params.default_values.len() {
                    params.default_values[di].clone()
                } else if di < params.defaults.len() {
                    eval_default_source(state, tools, &params.defaults[di])
                        .await
                        .unwrap_or(Value::None)
                } else {
                    return Err(InterpreterError::TypeError(format!(
                        "missing required argument: '{}'",
                        param.name
                    ))
                    .into());
                };
                scope.insert(param.name.clone(), default_val);
            } else {
                return Err(InterpreterError::TypeError(format!(
                    "missing required argument: '{}'",
                    param.name
                ))
                .into());
            }
        }
    }

    // Bind *args
    if let Some(ref vararg_name) = params.vararg {
        let extra: Vec<Value> = args.iter().skip(num_params).cloned().collect();
        scope.insert(vararg_name.clone(), Value::Tuple(extra));
    }

    // Bind keyword-only arguments. Same def-time-vs-fallback shape
    // as positional defaults.
    for (i, kw_param) in params.kwonlyargs.iter().enumerate() {
        if let Some(val) = kwargs.get(&kw_param.name) {
            scope.insert(kw_param.name.clone(), val.clone());
        } else if let Some(Some(default_val)) = params.kw_default_values.get(i) {
            scope.insert(kw_param.name.clone(), default_val.clone());
        } else if let Some(Some(default_src)) = params.kw_defaults.get(i) {
            let default_val =
                eval_default_source(state, tools, default_src).await.unwrap_or(Value::None);
            scope.insert(kw_param.name.clone(), default_val);
        } else {
            return Err(InterpreterError::TypeError(format!(
                "missing required keyword argument: '{}'",
                kw_param.name
            ))
            .into());
        }
    }

    // Bind **kwargs
    if let Some(ref kwarg_name) = params.kwarg {
        let mut extra_kwargs = IndexMap::new();
        let param_names: Vec<&str> =
            params.args.iter().chain(params.kwonlyargs.iter()).map(|p| p.name.as_str()).collect();
        for (k, v) in kwargs {
            if !param_names.contains(&k.as_str()) {
                extra_kwargs.insert(ValueKey::String(k.clone().into()), v.clone());
            }
        }
        scope.insert(kwarg_name.clone(), Value::Dict(extra_kwargs));
    }

    Ok(scope)
}

/// Execute a function body, catching Return signals.
pub(crate) async fn execute_body(
    state: &mut InterpreterState,
    body: &[ast::Stmt],
    tools: &Tools,
) -> EvalResult {
    let mut result = Value::None;
    for stmt in body {
        match eval_stmt(state, stmt, tools).await {
            Ok(val) => result = val,
            Err(EvalError::Signal(ControlFlow::Return(val))) => return Ok(*val),
            Err(e) => return Err(e),
        }
    }
    Ok(result)
}
