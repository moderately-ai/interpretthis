// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A sequential emulation of `asyncio` for the synchronous sandbox.
//!
//! Coroutines (from `async def`) are driven to completion on demand rather than
//! scheduled on an event loop, so `async def` / `await` / `asyncio.run` and the
//! data-flow helpers (`gather`, `sleep`, `create_task`, `ensure_future`,
//! `wait_for`) produce CPython-identical *results*. What is NOT reproduced is
//! true concurrency: `gather`/`create_task` run their coroutines one after
//! another rather than interleaving at `await` points, so output whose ordering
//! depends on the event loop's scheduling can differ. Real timers/IO are absent
//! (`sleep` is a no-op), matching the sandbox's determinism.

use indexmap::IndexMap;

use crate::{
    error::{EvalResult, InterpreterError},
    state::InterpreterState,
    tools::Tools,
    value::Value,
};

pub struct AsyncioModule;

/// Drive one awaitable to its result: a coroutine runs its body to completion,
/// anything already-resolved (a plain value produced eagerly) passes through.
async fn resolve(state: &mut InterpreterState, awaitable: Value, tools: &Tools) -> EvalResult {
    match awaitable {
        Value::Coroutine(coro) => {
            crate::eval::functions::drive_coroutine(state, &coro, tools).await
        }
        other => Ok(other),
    }
}

#[async_trait::async_trait]
impl crate::eval::modules::Module for AsyncioModule {
    fn name(&self) -> &'static str {
        "asyncio"
    }

    fn has_function(&self, name: &str) -> bool {
        matches!(name, "run" | "gather" | "sleep" | "create_task" | "ensure_future" | "wait_for")
    }

    async fn call(
        &self,
        state: &mut InterpreterState,
        func: &str,
        args: &[Value],
        kwargs: &IndexMap<String, Value>,
        tools: &Tools,
    ) -> EvalResult {
        match func {
            // `asyncio.run(coro)` drives the top-level coroutine to completion.
            "run" => {
                let coro = args.first().cloned().ok_or_else(|| {
                    InterpreterError::TypeError("run() missing required argument 'main'".into())
                })?;
                resolve(state, coro, tools).await
            }
            // `gather(*aws)` resolves each awaitable in order, returning the list
            // of results. `return_exceptions=` is accepted but, with no real
            // task isolation, a raised exception still propagates.
            "gather" => {
                let mut results = Vec::with_capacity(args.len());
                for aw in args {
                    results.push(resolve(state, aw.clone(), tools).await?);
                }
                Ok(Value::List(crate::value::shared_list(results)))
            }
            // `sleep(delay, result=None)` — no real timer; yields `result`.
            "sleep" => {
                let result =
                    args.get(1).or_else(|| kwargs.get("result")).cloned().unwrap_or(Value::None);
                Ok(result)
            }
            // `create_task`/`ensure_future(coro)` — return the coroutine to be
            // driven when awaited (this sandbox has no background scheduler).
            "create_task" | "ensure_future" => args.first().cloned().ok_or_else(|| {
                InterpreterError::TypeError(format!("{func}() missing required argument")).into()
            }),
            // `wait_for(coro, timeout)` — no timeout enforcement; drive the coro.
            "wait_for" => {
                let coro = args.first().cloned().ok_or_else(|| {
                    InterpreterError::TypeError("wait_for() missing required argument 'fut'".into())
                })?;
                resolve(state, coro, tools).await
            }
            _ => Err(InterpreterError::AttributeError(format!(
                "module 'asyncio' has no callable '{func}'"
            ))
            .into()),
        }
    }
}
