// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `functools` module.
//!
//! Supports `reduce` (call-back into the evaluator's `call_user_function`
//! / `call_lambda` for each pair) and `wraps` (no-op identity decorator
//! — CPython's wraps copies metadata; we approximate as identity since
//! our FunctionDef metadata isn't observable beyond `__name__`).
//!
//! Other functools entries (`partial`, `lru_cache`, `cache`,
//! `singledispatch`, `cached_property`, `cmp_to_key`) land as a
//! follow-up — partial / lru_cache need a fresh Value variant to
//! capture the bound state, and cmp_to_key needs the cmp callback
//! threaded into sorted()'s sort path.

use indexmap::IndexMap;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::control_flow::iterate_value,
    state::InterpreterState,
    tools::Tools,
    value::Value,
};

pub fn has_function(name: &str) -> bool {
    matches!(name, "wraps" | "reduce" | "partial")
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
        "wraps" => {
            // wraps(wrapped) -> identity decorator. CPython's wraps
            // returns a decorator that copies metadata from `wrapped`
            // onto the decorated function. We approximate with a
            // no-op identity: wraps(_) returns a lambda `x -> x` so
            // `@wraps(f) def g: ...` reduces to `g = (lambda x: x)(g)
            // = g`. The metadata-copy semantics aren't observable in
            // this interpreter beyond FunctionDef.name.
            //
            // Construct the identity lambda by registering an `x`
            // body in state.lambda_bodies under a synthetic key, then
            // returning a LambdaDef pointing to it. The key is
            // shared across calls (the body is the same expression).
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "wraps() missing required argument".into(),
                )
                .into());
            }
            let key = "__functools_wraps_identity__";
            if !state.lambda_bodies.contains_key(key) {
                state.lambda_bodies.insert(
                    key.to_string(),
                    std::sync::Arc::new(rustpython_parser::ast::Expr::Name(
                        rustpython_parser::ast::ExprName {
                            id: rustpython_parser::ast::Identifier::new("x"),
                            ctx: rustpython_parser::ast::ExprContext::Load,
                            range: rustpython_parser::text_size::TextRange::default(),
                        },
                    )),
                );
            }
            Ok(Value::Lambda(std::sync::Arc::new(crate::value::LambdaDef {
                params: crate::value::FunctionParams {
                    args: vec![crate::value::Param { name: "x".to_string() }],
                    defaults: Vec::new(),
                    default_values: Vec::new(),
                    vararg: None,
                    kwonlyargs: Vec::new(),
                    kw_defaults: Vec::new(),
                    kw_default_values: Vec::new(),
                    kwarg: None,
                },
                lambda_id: key.to_string(),
                source: "lambda x: x".to_string(),
                closure: std::collections::BTreeMap::new(),
                assigned_names: Vec::new(),
                // Synthesized lambda; treat as module-level to avoid
                // applying its empty closure as an overlay.
                is_module_level: true,
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
