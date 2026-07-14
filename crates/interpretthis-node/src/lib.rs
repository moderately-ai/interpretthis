// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Node.js bindings for `interpretthis` — run untrusted or LLM-generated Python
//! inside a sandbox, from JavaScript.

#![allow(clippy::needless_pass_by_value, reason = "napi takes its arguments by value")]

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use interpretthis::{
    Interpreter, InterpreterConfig, InterpreterDeps, InterpreterError, ToolDefinition, Tools, Value,
};
use napi::{
    Env, Error, Result, Status,
    bindgen_prelude::{Buffer, Unknown},
};
use napi_derive::napi;

mod convert;
mod tools;

use convert::{SandboxValue, value_to_js};
use tools::{JsToolHandler, ToolEntry};

/// Wire format of `exportState` blobs. Independent of the package version: a blob
/// is portable between builds that agree on this number, and no others.
#[napi]
pub const STATE_FORMAT_VERSION: u32 = interpretthis::STATE_FORMAT_VERSION;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Resource limits. Every field is optional and falls back to the interpreter's
/// own default, so `{ maxOperations: 1000 }` tightens one limit and leaves the
/// rest alone.
#[napi(object)]
pub struct Config {
    pub max_operations: Option<i64>,
    pub max_while_iterations: Option<i64>,
    pub max_memory_bytes: Option<i64>,
    pub max_stdout_bytes: Option<i64>,
    // i64 (not u32): a JS `-1` would otherwise be coerced by ECMAScript
    // ToUint32 to 4294967295, silently disabling the limit. As i64 it is
    // range-validated below.
    pub max_concurrent_tools: Option<i64>,
    /// Wall-clock budget in **seconds**. Omit for no limit.
    pub max_execution_time: Option<f64>,
    pub max_recursion_depth: Option<i64>,
    pub max_int_bits: Option<i64>,
}

fn positive(field: &str, value: i64) -> Result<u64> {
    u64::try_from(value)
        .map_err(|_| Error::new(Status::InvalidArg, format!("{field} must not be negative")))
}

/// A non-negative value that fits `u32` — rejects a negative (which JS ToUint32
/// would wrap into a huge limit) and an out-of-range value.
fn positive_u32(field: &str, value: i64) -> Result<u32> {
    u32::try_from(value).map_err(|_| {
        Error::new(Status::InvalidArg, format!("{field} must be between 0 and {}", u32::MAX))
    })
}

impl Config {
    fn into_interpreter_config(self) -> Result<InterpreterConfig> {
        // InterpreterConfig is #[non_exhaustive]: it cannot be built with a
        // struct literal from outside its crate. Mutating the default is the
        // supported route.
        let mut config = InterpreterConfig::default();

        if let Some(v) = self.max_operations {
            config.max_operations = positive("maxOperations", v)?;
        }
        if let Some(v) = self.max_while_iterations {
            config.max_while_iterations = positive("maxWhileIterations", v)?;
        }
        if let Some(v) = self.max_memory_bytes {
            config.max_memory_bytes = positive("maxMemoryBytes", v)?;
        }
        if let Some(v) = self.max_stdout_bytes {
            config.max_stdout_bytes = positive("maxStdoutBytes", v)?;
        }
        if let Some(v) = self.max_concurrent_tools {
            config.max_concurrent_tools = positive_u32("maxConcurrentTools", v)?;
        }
        if let Some(secs) = self.max_execution_time {
            if !secs.is_finite() || secs < 0.0 {
                return Err(Error::new(
                    Status::InvalidArg,
                    "maxExecutionTime must be a non-negative, finite number of seconds",
                ));
            }
            config.max_execution_time = Some(Duration::from_secs_f64(secs));
        }
        if let Some(v) = self.max_recursion_depth {
            config.max_recursion_depth = positive_u32("maxRecursionDepth", v)?;
        }
        if let Some(v) = self.max_int_bits {
            config.max_int_bits = positive("maxIntBits", v)?;
        }

        Ok(config)
    }
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ExecutionResult
// ---------------------------------------------------------------------------

/// The outcome of one run.
///
/// Deliberately a value, not a thrown error. A failing run is *data*: `stdout` is
/// populated even when `error` is set (a script that prints three lines and then
/// throws gives you all three), and that pair is exactly what gets fed back to a
/// model. `check()` throws where that is wanted.
#[napi(object)]
pub struct ExecutionResult {
    /// Everything the script printed, including output produced before a failure.
    pub stdout: String,
    /// True when the script ran to completion.
    pub ok: bool,
    /// How it failed, or `null`.
    pub error: Option<ExecutionError>,
}

/// A structured failure. `kind` is stable and switchable; `message` is for humans.
#[napi(object)]
pub struct ExecutionError {
    /// One of: `syntax`, `security`, `runtime`, `limitExceeded`,
    /// `recursionLimit`, `tool`, `name`, `type`, `value`, `attribute`,
    /// `assertion`, `exception`, `stateFormat`, `other`.
    pub kind: String,
    pub message: String,
    /// Set when `kind` is `tool`: the tool that failed.
    pub tool_name: Option<String>,
    /// Set when `kind` is `exception`: the exception class as the *script* saw
    /// it — including classes the script defined itself.
    pub type_name: Option<String>,
}

impl From<&InterpreterError> for ExecutionError {
    fn from(err: &InterpreterError) -> Self {
        let message = err.to_string();
        let (kind, tool_name, type_name) = match err {
            InterpreterError::Syntax(_) => ("syntax", None, None),
            InterpreterError::Security(_) => ("security", None, None),
            InterpreterError::Runtime(_) => ("runtime", None, None),
            InterpreterError::LimitExceeded(_) => ("limitExceeded", None, None),
            InterpreterError::RecursionLimitExceeded { .. } => ("recursionLimit", None, None),
            InterpreterError::Tool { tool_name, .. } => ("tool", Some(tool_name.clone()), None),
            InterpreterError::NameError(_) => ("name", None, None),
            InterpreterError::TypeError(_) => ("type", None, None),
            InterpreterError::ValueError(_) => ("value", None, None),
            InterpreterError::AttributeError(_) => ("attribute", None, None),
            InterpreterError::AssertionError(_) => ("assertion", None, None),
            InterpreterError::PythonException { type_name, .. } => {
                ("exception", None, Some(type_name.clone()))
            }
            InterpreterError::StateFormatSuperseded { .. } => ("stateFormat", None, None),
            // InterpreterError is #[non_exhaustive]: a new variant surfaces with
            // its message rather than vanishing.
            _ => ("other", None, None),
        };

        Self { kind: kind.to_string(), message, tool_name, type_name }
    }
}

// ---------------------------------------------------------------------------
// Interpreter
// ---------------------------------------------------------------------------

/// A sandboxed Python interpreter with host tool injection.
#[napi(js_name = "Interpreter")]
pub struct JsInterpreter {
    inner: Arc<Interpreter>,
    registered: Arc<Tools>,
    running: Arc<AtomicBool>,
}

/// Clears the busy flag however the run ends, including if the caller drops the
/// promise.
struct RunGuard(Arc<AtomicBool>);

impl Drop for RunGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Read a `{ name: fn | { func, parallelizable } }` object into a tool set.
///
/// Names are validated here, at registration, not at first call: a blocked name
/// (`eval`, `exec`, `os`, ...) should fail loudly when you build the interpreter
/// rather than silently do nothing until some script happens to call it.
fn read_tools(tools: Option<HashMap<String, ToolEntry>>) -> Result<Tools> {
    let mut built = Tools::new();
    let Some(tools) = tools else {
        return Ok(built);
    };

    for (name, entry) in tools {
        let handler = JsToolHandler::new(entry.func);
        let definition = if entry.parallelizable {
            ToolDefinition::parallel(handler)
        } else {
            ToolDefinition::new(handler)
        };

        // `Tools::try_insert` owns the name policy; asking it is what keeps this
        // binding from carrying a second, drifting copy of the blocklist. Names
        // are checked at registration, not at first call: a blocked name
        // (`eval`, `exec`, `os`, ...) should fail loudly when you build the
        // interpreter rather than silently do nothing until a script calls it.
        built
            .try_insert(&name, definition)
            .map_err(|e| Error::new(Status::InvalidArg, e.message))?;
    }

    Ok(built)
}

#[napi]
impl JsInterpreter {
    /// Build an interpreter.
    ///
    /// Tool names are validated now, so a blocked name throws here rather than
    /// silently failing to resolve later.
    #[napi(constructor)]
    pub fn new(tools: Option<HashMap<String, ToolEntry>>, config: Option<Config>) -> Result<Self> {
        let registered = read_tools(tools)?;
        let config = config
            .map_or_else(|| Ok(InterpreterConfig::default()), Config::into_interpreter_config)?;

        Ok(Self {
            inner: Arc::new(Interpreter::new(InterpreterDeps { tools: Tools::new() }, config)),
            registered: Arc::new(registered),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Claim the interpreter for one run.
    ///
    /// Refuses if a run is already in progress. That covers the case that would
    /// otherwise be a silent hang: `execute` called *from inside a tool
    /// callback*. The interpreter holds its state mutex across the whole run —
    /// including across the await for a tool — and that mutex is not reentrant,
    /// so a nested call blocks forever, holding up the very tool whose completion
    /// it is waiting on. Nothing times out; nothing is logged.
    fn begin_run(&self) -> Result<RunGuard> {
        self.running.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).map_err(
            |_| {
                Error::new(
                    Status::GenericFailure,
                    "this Interpreter is already running. Calling execute() from inside a tool \
                     callback would deadlock (the interpreter holds its state lock across the \
                     whole run). Use a separate Interpreter for each concurrent run.",
                )
            },
        )?;
        Ok(RunGuard(Arc::clone(&self.running)))
    }

    /// Run `code`, resolving to an `ExecutionResult`.
    ///
    /// Async by necessity, not for style: a JS tool callback can only resolve
    /// while the event loop is free, so the run happens on napi's tokio runtime
    /// and the loop stays available to service tools.
    #[napi]
    pub async fn execute(
        &self,
        code: String,
        variables: Option<HashMap<String, SandboxValue>>,
        tools: Option<HashMap<String, ToolEntry>>,
    ) -> Result<ExecutionResult> {
        let guard = self.begin_run()?;

        let variables: HashMap<String, Value> =
            variables.unwrap_or_default().into_iter().map(|(k, v)| (k, v.0)).collect();

        let per_call = read_tools(tools)?;
        // Per-call tools win on a name clash, matching the Rust API.
        let effective = self.registered.merged_with(&per_call);

        let inner = Arc::clone(&self.inner);
        let response = async move {
            let _guard = guard;
            inner.execute(&code, &effective, variables).await
        }
        .await;

        Ok(ExecutionResult {
            ok: response.error.is_none(),
            error: response.error.as_ref().map(ExecutionError::from),
            stdout: response.stdout,
        })
    }

    /// Read a variable out of the interpreter's state.
    ///
    /// `null` both when the name is unset and when it holds Python's `None`; use
    /// `stateKeys()` to tell those apart.
    #[napi]
    pub fn get_variable<'env>(
        &self,
        env: &'env Env,
        name: String,
    ) -> Result<Option<Unknown<'env>>> {
        self.inner.get_variable(&name).map(|value| value_to_js(env, &value)).transpose()
    }

    /// Names currently bound in the interpreter's state.
    #[napi]
    pub fn state_keys(&self) -> Vec<String> {
        self.inner.state_keys()
    }

    /// Bytes the interpreter has accounted for — the counter gating
    /// `maxMemoryBytes`. Not RSS.
    #[napi]
    pub fn accounted_bytes(&self) -> i64 {
        i64::try_from(self.inner.accounted_bytes()).unwrap_or(i64::MAX)
    }

    /// Serialise variables and classes for later resume. Signing and encryption
    /// are yours to do.
    #[napi]
    pub fn export_state(&self) -> Result<Buffer> {
        self.inner
            .export_state()
            .map(Buffer::from)
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
    }

    /// Restore state from `exportState` bytes.
    ///
    /// Throws if the blob came from an interpreter with a different state
    /// format; the interpreter never silently migrates one.
    #[napi]
    pub fn import_state(&self, data: Buffer) -> Result<()> {
        self.inner
            .import_state(&data)
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
    }
}
