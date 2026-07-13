// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Python bindings for `interpretthis` — run untrusted or LLM-generated Python
//! inside a sandbox, from Python.
//!
//! The compiled module is `interpretthis._native`. Everything user-facing is
//! re-exported by the `interpretthis` package that wraps it; the exception
//! classes in particular are defined there, in Python, and imported here (see
//! `errors.rs`).

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use interpretthis::{
    Interpreter, InterpreterConfig, InterpreterDeps, InterpreterError, InterpreterResponse,
    STATE_FORMAT_VERSION, ToolDefinition, ToolError, ToolHandler, Tools, Value,
};
use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
    types::{PyBytes, PyDict},
};
use pyo3_async_runtimes::TaskLocals;

mod convert;
mod errors;
mod tools;

use convert::{py_to_value, value_to_py};
use tools::{BackgroundLoop, LoopSource, PyToolHandler};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Resource limits for an [`Interpreter`].
///
/// Mirrors `InterpreterConfig`. Every argument is optional and defaults to the
/// interpreter's own default, so `Config(max_operations=1000)` tightens one
/// limit and leaves the rest alone.
// `from_py_object` is an explicit opt-in in pyo3 0.29 (it used to be implied by
// `Clone`). We want it: `Interpreter(config=...)` takes a Config by value.
#[pyclass(name = "Config", module = "interpretthis._native", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyConfig {
    inner: InterpreterConfig,
}

#[pymethods]
impl PyConfig {
    #[new]
    #[pyo3(signature = (
        max_operations = None,
        max_while_iterations = None,
        max_memory_bytes = None,
        max_stdout_bytes = None,
        max_concurrent_tools = None,
        max_execution_time = None,
        max_recursion_depth = None,
        max_int_bits = None,
    ))]
    #[expect(clippy::too_many_arguments, reason = "one argument per configurable limit")]
    fn new(
        max_operations: Option<u64>,
        max_while_iterations: Option<u64>,
        max_memory_bytes: Option<u64>,
        max_stdout_bytes: Option<u64>,
        max_concurrent_tools: Option<u32>,
        max_execution_time: Option<f64>,
        max_recursion_depth: Option<u32>,
        max_int_bits: Option<u64>,
    ) -> PyResult<Self> {
        // InterpreterConfig is #[non_exhaustive], so it cannot be built with a
        // struct literal or functional-update syntax from outside its crate.
        // Mutating the default is the supported route.
        let mut inner = InterpreterConfig::default();

        if let Some(v) = max_operations {
            inner.max_operations = v;
        }
        if let Some(v) = max_while_iterations {
            inner.max_while_iterations = v;
        }
        if let Some(v) = max_memory_bytes {
            inner.max_memory_bytes = v;
        }
        if let Some(v) = max_stdout_bytes {
            inner.max_stdout_bytes = v;
        }
        if let Some(v) = max_concurrent_tools {
            inner.max_concurrent_tools = v;
        }
        if let Some(secs) = max_execution_time {
            if !secs.is_finite() || secs < 0.0 {
                return Err(PyValueError::new_err(
                    "max_execution_time must be a non-negative, finite number of seconds",
                ));
            }
            inner.max_execution_time = Some(Duration::from_secs_f64(secs));
        }
        if let Some(v) = max_recursion_depth {
            inner.max_recursion_depth = v;
        }
        if let Some(v) = max_int_bits {
            inner.max_int_bits = v;
        }

        Ok(Self { inner })
    }

    #[getter]
    const fn max_operations(&self) -> u64 {
        self.inner.max_operations
    }
    #[getter]
    const fn max_while_iterations(&self) -> u64 {
        self.inner.max_while_iterations
    }
    #[getter]
    const fn max_memory_bytes(&self) -> u64 {
        self.inner.max_memory_bytes
    }
    #[getter]
    const fn max_stdout_bytes(&self) -> u64 {
        self.inner.max_stdout_bytes
    }
    #[getter]
    const fn max_concurrent_tools(&self) -> u32 {
        self.inner.max_concurrent_tools
    }
    #[getter]
    fn max_execution_time(&self) -> Option<f64> {
        self.inner.max_execution_time.map(|d| d.as_secs_f64())
    }
    #[getter]
    const fn max_recursion_depth(&self) -> u32 {
        self.inner.max_recursion_depth
    }
    #[getter]
    const fn max_int_bits(&self) -> u64 {
        self.inner.max_int_bits
    }
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

/// A tool with non-default settings.
///
/// A bare callable in the `tools` mapping is a sequential tool. Wrap it in
/// `Tool(fn, parallelizable=True)` to let the interpreter run it concurrently
/// with other parallelizable tools — only safe if the tool has no
/// order-dependent side effects.
#[pyclass(name = "Tool", module = "interpretthis._native", frozen)]
pub struct PyTool {
    func: Py<PyAny>,
    parallelizable: bool,
}

#[pymethods]
impl PyTool {
    #[new]
    #[pyo3(signature = (func, *, parallelizable = false))]
    fn new(func: Py<PyAny>, parallelizable: bool) -> Self {
        Self { func, parallelizable }
    }

    #[getter]
    fn func(&self, py: Python<'_>) -> Py<PyAny> {
        self.func.clone_ref(py)
    }

    #[getter]
    const fn parallelizable(&self) -> bool {
        self.parallelizable
    }
}

/// A registered tool, as the interpreter wrapper stores it.
///
/// Stored as the raw callable rather than a built `Tools` because the handler
/// needs an event loop source, and *which* loop depends on whether this run came
/// through `execute` or `execute_async`. See `tools.rs`.
struct ToolSpec {
    func: Py<PyAny>,
    parallelizable: bool,
}

/// Placeholder used to validate a tool name at registration time without
/// building a real handler. Never invoked: `Tools` is rebuilt per execute.
struct UnusedHandler;

#[async_trait::async_trait]
impl ToolHandler for UnusedHandler {
    async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        Err(ToolError::new("internal: placeholder tool handler was invoked"))
    }
}

/// Read a `{name: callable | Tool}` mapping into tool specs, rejecting names the
/// sandbox will not allow.
///
/// Names are validated here, at registration, rather than at first call: a typo
/// or a blocked name (`eval`, `exec`, `os`, ...) should fail loudly when the
/// interpreter is built, not silently do nothing until some script happens to
/// call it.
fn read_tool_specs(
    py: Python<'_>,
    mapping: Option<&Bound<'_, PyDict>>,
) -> PyResult<HashMap<String, ToolSpec>> {
    let mut specs = HashMap::new();
    let Some(mapping) = mapping else {
        return Ok(specs);
    };

    // `Tools::try_insert` owns the name policy; asking it is what keeps this
    // binding from carrying a second, drifting copy of the blocklist.
    let mut validator = Tools::new();

    for (key, value) in mapping {
        let name: String =
            key.extract().map_err(|_| PyValueError::new_err("tool names must be strings"))?;

        validator
            .try_insert(&name, ToolDefinition::new(UnusedHandler))
            .map_err(|e| PyValueError::new_err(e.message))?;

        let spec = if let Ok(tool) = value.cast::<PyTool>() {
            let tool = tool.get();
            ToolSpec { func: tool.func.clone_ref(py), parallelizable: tool.parallelizable }
        } else if value.is_callable() {
            ToolSpec { func: value.clone().unbind(), parallelizable: false }
        } else {
            return Err(PyValueError::new_err(format!(
                "tool '{name}' must be a callable or a Tool, not {}",
                value.get_type().name()?
            )));
        };

        specs.insert(name, spec);
    }

    Ok(specs)
}

/// Build the interpreter's `Tools` for one run, binding every handler to this
/// run's event loop source.
fn build_tools(
    py: Python<'_>,
    registered: &HashMap<String, ToolSpec>,
    per_call: &HashMap<String, ToolSpec>,
    loop_source: &Arc<LoopSource>,
) -> PyResult<Tools> {
    let mut tools = Tools::new();

    // Per-call tools win on a name clash, matching the Rust API.
    for (name, spec) in registered.iter().chain(per_call.iter()) {
        let handler = PyToolHandler::new(spec.func.clone_ref(py), Arc::clone(loop_source));
        let definition = if spec.parallelizable {
            ToolDefinition::parallel(handler)
        } else {
            ToolDefinition::new(handler)
        };
        tools.try_insert(name, definition).map_err(|e| PyValueError::new_err(e.message))?;
    }

    Ok(tools)
}

fn read_variables(mapping: Option<&Bound<'_, PyDict>>) -> PyResult<HashMap<String, Value>> {
    let mut variables = HashMap::new();
    let Some(mapping) = mapping else {
        return Ok(variables);
    };
    for (key, value) in mapping {
        let name: String =
            key.extract().map_err(|_| PyValueError::new_err("variable names must be strings"))?;
        variables.insert(name, py_to_value(&value)?);
    }
    Ok(variables)
}

// ---------------------------------------------------------------------------
// ExecutionResult
// ---------------------------------------------------------------------------

/// The outcome of one `execute`.
///
/// Deliberately a value, not an exception. A failing run is *data*: the whole
/// point of this library is to hand a model back what its code printed and how
/// it broke, and `stdout` is populated even when `error` is set (a script that
/// prints three lines and then raises gives you all three). Raising would force
/// every caller into a `try` block to recover output they always want.
///
/// Call `check()` for the raising behaviour where it is wanted.
#[pyclass(name = "ExecutionResult", module = "interpretthis._native", frozen)]
pub struct PyExecutionResult {
    #[pyo3(get)]
    stdout: String,
    error: Option<InterpreterError>,
}

impl From<InterpreterResponse> for PyExecutionResult {
    fn from(response: InterpreterResponse) -> Self {
        Self { stdout: response.stdout, error: response.error }
    }
}

#[pymethods]
impl PyExecutionResult {
    /// True when the script ran to completion.
    #[getter]
    const fn ok(&self) -> bool {
        self.error.is_none()
    }

    /// The exception instance describing the failure, or `None`.
    ///
    /// An *instance*, not a raised exception — inspect `.tool_name`,
    /// `.type_name`, and friends without a `try` block.
    #[getter]
    fn error(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.error.as_ref().map(|e| errors::to_pyerr(e).value(py).clone().into_any().unbind())
    }

    /// Raise if the run failed; return `self` otherwise, so it chains.
    fn check(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        match &slf.error {
            Some(err) => Err(errors::to_pyerr(err)),
            None => Ok(slf),
        }
    }

    fn __repr__(&self) -> String {
        match &self.error {
            None => format!("ExecutionResult(ok=True, stdout={:?})", self.stdout),
            Some(err) => format!("ExecutionResult(ok=False, error={:?})", err.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Interpreter
// ---------------------------------------------------------------------------

/// A sandboxed Python interpreter with host tool injection.
#[pyclass(name = "Interpreter", module = "interpretthis._native", frozen)]
pub struct PyInterpreter {
    inner: Arc<Interpreter>,
    registered: HashMap<String, ToolSpec>,
    background: Arc<BackgroundLoop>,
    /// Guards against overlapping runs on one interpreter — see `begin_run`.
    running: Arc<AtomicBool>,
}

/// Clears the busy flag however the run ends, including on a Python exception
/// or a cancelled `execute_async` future.
struct RunGuard(Arc<AtomicBool>);

impl Drop for RunGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl PyInterpreter {
    /// Claim the interpreter for one run.
    ///
    /// Refuses if a run is already in progress on this object. That covers the
    /// case that would otherwise be a silent hang: `execute` called *from inside
    /// a tool callback*. The interpreter holds its state mutex across the whole
    /// run — including across the await for a tool — and the mutex is not
    /// reentrant, so a nested call blocks forever, holding up the very tool
    /// whose completion it is waiting on. No timeout fires; nothing is logged.
    ///
    /// This is stricter than the Rust API, where concurrent `execute` calls
    /// simply queue on that mutex. Concurrency on a *single* interpreter is not
    /// meaningful anyway — runs share one variable namespace, so overlapping
    /// them interleaves state. Use one `Interpreter` per concurrent run; they
    /// are cheap and isolated by design.
    fn begin_run(&self) -> PyResult<RunGuard> {
        self.running.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).map_err(
            |_| {
                PyRuntimeError::new_err(
                    "this Interpreter is already running. Calling execute() from inside a tool \
                     callback would deadlock (the interpreter holds its state lock across the \
                     whole run), and two threads sharing one Interpreter would interleave its \
                     variables. Use a separate Interpreter for each concurrent run.",
                )
            },
        )?;
        Ok(RunGuard(Arc::clone(&self.running)))
    }
}

#[pymethods]
impl PyInterpreter {
    /// Build an interpreter.
    ///
    /// `tools` maps a name to a callable — sync `def` or `async def` — or to a
    /// `Tool` for non-default settings. Tool names are validated now, so a
    /// blocked name (`eval`, `exec`, `os`, ...) raises here rather than
    /// silently failing to resolve later.
    #[new]
    #[pyo3(signature = (*, tools = None, config = None))]
    fn new(
        py: Python<'_>,
        tools: Option<&Bound<'_, PyDict>>,
        config: Option<PyConfig>,
    ) -> PyResult<Self> {
        let registered = read_tool_specs(py, tools)?;
        let config = config.map_or_else(InterpreterConfig::default, |c| c.inner);

        // Registered tools are held here, not handed to the interpreter: the
        // real `Tools` is rebuilt per run so each handler gets that run's event
        // loop. The interpreter's own registry stays empty and every tool is
        // passed as a per-call tool, which the core merges identically.
        let inner = Interpreter::new(InterpreterDeps { tools: Tools::new() }, config);

        Ok(Self {
            inner: Arc::new(inner),
            registered,
            background: Arc::new(BackgroundLoop::default()),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Run `code`, returning an `ExecutionResult`.
    ///
    /// Blocks. Tool callbacks may be sync or `async def`; coroutine tools are
    /// driven on a dedicated background event loop this interpreter starts on
    /// first use. Use `execute_async` from async code.
    #[pyo3(signature = (code, /, variables = None, *, tools = None))]
    fn execute(
        &self,
        py: Python<'_>,
        code: &str,
        variables: Option<&Bound<'_, PyDict>>,
        tools: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyExecutionResult> {
        let _guard = self.begin_run()?;

        let variables = read_variables(variables)?;
        let per_call = read_tool_specs(py, tools)?;
        let loop_source = Arc::new(LoopSource::Background(Arc::clone(&self.background)));
        let tools = build_tools(py, &self.registered, &per_call, &loop_source)?;

        let inner = Arc::clone(&self.inner);
        let runtime = pyo3_async_runtimes::tokio::get_runtime();

        // Release the GIL for the whole run. This is load-bearing, not an
        // optimisation: a tool callback runs on a tokio worker and must take the
        // GIL to call back into Python. Holding it here would deadlock the first
        // tool call.
        let response = py.detach(|| {
            runtime.block_on(async move { inner.execute(code, &tools, variables).await })
        });

        Ok(response.into())
    }

    /// Run `code` on the caller's event loop, returning an awaitable.
    ///
    /// Coroutine tools are scheduled on **the caller's** loop, so a tool may
    /// safely await objects bound to it (an `aiohttp` session, an
    /// `asyncio.Lock`).
    #[pyo3(signature = (code, /, variables = None, *, tools = None))]
    fn execute_async<'py>(
        &self,
        py: Python<'py>,
        code: String,
        variables: Option<&Bound<'_, PyDict>>,
        tools: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let guard = self.begin_run()?;

        let variables = read_variables(variables)?;
        let per_call = read_tool_specs(py, tools)?;

        // Captured here, on the caller's thread, while their loop is running —
        // it cannot be discovered later from a tokio worker.
        let locals = TaskLocals::with_running_loop(py)?.copy_context(py)?;
        let loop_source = Arc::new(LoopSource::Caller(locals));
        let tools = build_tools(py, &self.registered, &per_call, &loop_source)?;

        let inner = Arc::clone(&self.inner);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Moved in, so the busy flag clears when the run ends — including if
            // the awaiting task is cancelled and this future is dropped.
            let _guard = guard;
            let response = inner.execute(&code, &tools, variables).await;
            Ok(PyExecutionResult::from(response))
        })
    }

    /// Read a variable out of the interpreter's state.
    ///
    /// Returns `None` both when the name is unset and when it holds Python's
    /// `None`; use `state_keys()` to tell them apart.
    fn get_variable(&self, py: Python<'_>, name: &str) -> PyResult<Option<Py<PyAny>>> {
        self.inner
            .get_variable(name)
            .map(|value| value_to_py(py, &value).map(Bound::unbind))
            .transpose()
    }

    /// Names currently bound in the interpreter's state.
    fn state_keys(&self) -> Vec<String> {
        self.inner.state_keys()
    }

    /// Bytes the interpreter has accounted for. Not RSS — this is the counter
    /// that gates `max_memory_bytes`.
    fn accounted_bytes(&self) -> usize {
        self.inner.accounted_bytes()
    }

    /// Allocator-reported resident bytes where available, else `accounted_bytes`.
    fn resident_bytes(&self) -> usize {
        self.inner.resident_bytes()
    }

    /// Serialise variables and classes to a versioned blob for later resume.
    ///
    /// Signing and encryption are the host's business. Pending tool results are
    /// omitted — they are not meaningful across a resume.
    fn export_state<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let blob = self.inner.export_state().map_err(|e| errors::to_pyerr(&e))?;
        Ok(PyBytes::new(py, &blob))
    }

    /// Restore state from `export_state` bytes.
    ///
    /// Raises `StateFormatError` if the blob was written by an interpreter with
    /// a different state format; the host should restart from a clean state
    /// rather than attempt a migration.
    fn import_state(&self, data: &[u8]) -> PyResult<()> {
        self.inner.import_state(data).map_err(|e| errors::to_pyerr(&e))
    }
}

// ---------------------------------------------------------------------------
// Module
// ---------------------------------------------------------------------------

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    // Wire format of `export_state` blobs. Independent of the package version:
    // a blob is portable between builds that agree on this number and no others.
    module.add("STATE_FORMAT_VERSION", STATE_FORMAT_VERSION)?;

    module.add_class::<PyConfig>()?;
    module.add_class::<PyTool>()?;
    module.add_class::<PyExecutionResult>()?;
    module.add_class::<PyInterpreter>()?;

    Ok(())
}
