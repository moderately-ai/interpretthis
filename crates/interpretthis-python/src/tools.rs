// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bridging a Python callable into [`ToolHandler`].
//!
//! `ToolHandler::call` is an `async fn` invoked from a tokio worker thread —
//! and, for a `parallelizable` tool, from inside `tokio::spawn`, so the returned
//! future must be `Send`. A Python callable is reachable from there because
//! `Py<PyAny>` is `Send + Sync`; the work is in what happens when that callable
//! returns a coroutine.
//!
//! # How an `async def` tool works
//!
//! A coroutine is inert: it needs an event loop to drive it. We do not have one
//! on a tokio worker. [`pyo3_async_runtimes::into_future_with_locals`] solves
//! exactly this — given [`TaskLocals`] holding *some* event loop, it schedules
//! the coroutine onto that loop with `call_soon_threadsafe` (legal from any
//! thread) and hands back a plain Rust future that is `Send`. So the handler
//! never drives Python; it hands the coroutine to a loop and awaits the result.
//!
//! # Which loop
//!
//! Deliberately not the same one in both entry points, and this is the reason
//! [`Tools`] is rebuilt on every execute rather than baked in at construction:
//!
//! - `execute_async` runs under the caller's asyncio loop, so tool coroutines
//!   are scheduled **there**. It has to be the caller's: async code routinely
//!   closes over objects bound to the running loop (an `aiohttp` session, an
//!   `asyncio.Lock`), and driving such a coroutine on a foreign loop misbehaves.
//! - Sync `execute` has no caller loop at all, so the interpreter lazily starts
//!   **one** dedicated loop on a background thread and uses that. One per
//!   interpreter, not one per call, so a connection pool a tool builds on first
//!   use survives across calls.
//!
//! # The GIL
//!
//! [`ToolHandler::call`] acquires the GIL, calls the function, and *releases it
//! before awaiting*. It must: the coroutine can only make progress on the event
//! loop's own thread, which needs the GIL to run. Holding it across the await
//! would deadlock. `execute()` releasing the GIL for the whole run (see
//! `lib.rs`) is what makes the acquire here possible in the first place.

use std::{collections::HashMap, future::Future, pin::Pin};

use async_trait::async_trait;
use interpretthis::{ToolError, ToolHandler, Value};
use pyo3::{prelude::*, sync::PyOnceLock, types::PyDict};
use pyo3_async_runtimes::TaskLocals;

use crate::convert::{py_to_value, value_to_py};

/// Render a Python exception as the tool failure the interpreter understands.
///
/// The interpreter turns this into a catchable `Exception` inside user code, or
/// an `InterpreterError::Tool` for the host if it escapes — so a raising tool is
/// a normal, recoverable outcome, not a crash.
fn tool_error(py: Python<'_>, err: &PyErr) -> ToolError {
    let kind = err
        .get_type(py)
        .name()
        .map_or_else(|_| "Exception".to_string(), |n| n.to_string_lossy().into_owned());
    ToolError::new(format!(
        "{kind}: {}",
        err.value(py)
            .str()
            .map_or_else(|_| "<unprintable>".to_string(), |s| s.to_string_lossy().into_owned())
    ))
}

/// The event loop used to drive coroutine tools when the host called the
/// synchronous `execute()` and therefore has no loop of its own.
///
/// Created at most once per interpreter, on first use — a host whose tools are
/// all synchronous never starts a thread.
pub struct BackgroundLoop(PyOnceLock<Py<PyAny>>);

// Hand-written: `PyOnceLock`'s derived `Default` carries a `T: Default` bound,
// and `Py<PyAny>` has no Default. The cell is empty either way.
impl Default for BackgroundLoop {
    fn default() -> Self {
        Self(PyOnceLock::new())
    }
}

impl BackgroundLoop {
    /// Task locals bound to this interpreter's background loop, starting the
    /// loop (and its thread) if this is the first coroutine tool to need it.
    pub fn task_locals(&self, py: Python<'_>) -> PyResult<TaskLocals> {
        let event_loop = self.0.get_or_try_init(py, || {
            let asyncio = py.import("asyncio")?;
            let event_loop = asyncio.call_method0("new_event_loop")?;

            // A daemon thread: it must not keep the process alive at exit. The
            // loop is never explicitly closed — an interpreter can be dropped
            // while a tool coroutine is in flight, and tearing the loop down
            // under it would be worse than letting the daemon thread die with
            // the process.
            let kwargs = PyDict::new(py);
            kwargs.set_item("target", event_loop.getattr("run_forever")?)?;
            kwargs.set_item("daemon", true)?;
            kwargs.set_item("name", "interpretthis-tools")?;

            let thread = py.import("threading")?.call_method("Thread", (), Some(&kwargs))?;
            thread.call_method0("start")?;

            Ok::<_, PyErr>(event_loop.unbind())
        })?;

        Ok(TaskLocals::new(event_loop.bind(py).clone()))
    }
}

/// Where a coroutine tool's event loop comes from, resolved lazily.
///
/// Lazily, because resolving it is not free: the background variant *starts a
/// thread*. A host whose tools are all plain `def`s must never pay for one, so
/// the loop is only demanded at the moment a tool actually hands back a
/// coroutine.
pub enum LoopSource {
    /// The caller's own running loop, captured by `execute_async`.
    Caller(TaskLocals),
    /// This interpreter's dedicated background loop, for the sync `execute`
    /// path where the caller has no loop of their own.
    Background(std::sync::Arc<BackgroundLoop>),
}

impl LoopSource {
    fn task_locals(&self, py: Python<'_>) -> PyResult<TaskLocals> {
        match self {
            Self::Caller(locals) => Ok(locals.clone()),
            Self::Background(background) => background.task_locals(py),
        }
    }
}

/// A host tool backed by a Python callable — sync `def` or `async def`.
pub struct PyToolHandler {
    func: Py<PyAny>,
    loop_source: std::sync::Arc<LoopSource>,
}

impl PyToolHandler {
    pub const fn new(func: Py<PyAny>, loop_source: std::sync::Arc<LoopSource>) -> Self {
        Self { func, loop_source }
    }
}

/// What calling the Python function produced, once the GIL has been given back.
enum Outcome {
    /// A plain `def` returned a value; nothing left to do.
    Ready(Value),
    /// An `async def` returned a coroutine, now scheduled on an event loop.
    /// `Send`, so it can be awaited from a tokio worker.
    Pending(Pin<Box<dyn Future<Output = PyResult<Py<PyAny>>> + Send>>),
}

#[async_trait]
impl ToolHandler for PyToolHandler {
    async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        // Everything touching Python happens inside this closure, which ends
        // before the await below — the GIL is not held across it.
        let outcome =
            Python::attach(|py| self.invoke(py, &kwargs).map_err(|e| tool_error(py, &e)))?;

        match outcome {
            Outcome::Ready(value) => Ok(value),
            Outcome::Pending(future) => {
                // GIL released. The coroutine runs on its event loop's thread,
                // which is free to take the GIL and make progress.
                let result = future.await;
                Python::attach(|py| match result {
                    Ok(obj) => py_to_value(obj.bind(py)).map_err(|e| tool_error(py, &e)),
                    Err(e) => Err(tool_error(py, &e)),
                })
            }
        }
    }
}

impl PyToolHandler {
    fn invoke(&self, py: Python<'_>, kwargs: &HashMap<String, Value>) -> PyResult<Outcome> {
        // Positional args arrive from the interpreter as "arg0", "arg1", ...;
        // keyword args keep their Python names. Both are passed through as
        // keywords, which is the documented tool calling convention.
        let py_kwargs = PyDict::new(py);
        for (name, value) in kwargs {
            py_kwargs.set_item(name, value_to_py(py, value)?)?;
        }

        let result = self.func.bind(py).call((), Some(&py_kwargs))?;

        // `inspect.isawaitable` rather than a `__await__` attribute probe: it is
        // the definition of awaitable, and it recognises coroutines, futures,
        // and `types.coroutine`-decorated generators alike.
        let awaitable = py.import("inspect")?.call_method1("isawaitable", (&result,))?;

        if awaitable.is_truthy()? {
            // Only now do we need a loop — which, on the sync path, is what
            // starts the background thread. A tool set of plain `def`s never
            // reaches here.
            let locals = self.loop_source.task_locals(py)?;
            let future = pyo3_async_runtimes::into_future_with_locals(&locals, result)?;
            Ok(Outcome::Pending(Box::pin(future)))
        } else {
            Ok(Outcome::Ready(py_to_value(&result)?))
        }
    }
}
