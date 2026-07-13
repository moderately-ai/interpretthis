// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bridging a JavaScript function into [`ToolHandler`].
//!
//! JavaScript is single-threaded and its values live on the JS thread;
//! `ToolHandler::call` runs on a tokio worker. `ThreadsafeFunction` is the
//! sanctioned crossing: it is `Send + Sync`, so it satisfies the trait's bounds
//! (including for a `parallelizable` tool, whose future gets `tokio::spawn`ed),
//! and `call_async` marshals the invocation onto the JS thread and awaits the
//! result.
//!
//! # One path for `function` and `async function`
//!
//! The return type is `Either<Promise<SandboxValue>, SandboxValue>`. napi tries each arm in
//! order, validating first, so a thenable becomes the `Promise` arm and a plain
//! value the other. Awaiting the promise resolves the tool; there is no separate
//! sync path to keep in step.
//!
//! # Why `execute` must be async
//!
//! A JS tool callback can only *run* when the event loop is free. `#[napi] async
//! fn execute` returns a Promise and does its work on napi's tokio runtime, so
//! the loop stays available to service these callbacks. A blocking `execute`
//! would deadlock the first async tool: the interpreter would wait on a promise
//! the loop is not running to resolve.

use std::collections::HashMap;

use async_trait::async_trait;
use interpretthis::{ToolError, ToolHandler, Value};
use napi::{
    Env, Error, JsValue as _, Result as NapiResult, Status, ValueType,
    bindgen_prelude::{
        Either, FromNapiValue, Function, JsObjectValue as _, Promise, TypeName, Unknown,
        ValidateNapiValue,
    },
    threadsafe_function::ThreadsafeFunction,
};

use crate::convert::SandboxValue;

/// Arguments handed to a JS tool: the interpreter's kwargs as a plain object.
///
/// Positional arguments from the script arrive as `arg0`, `arg1`, ...; keyword
/// arguments keep their names. That is the interpreter's tool calling
/// convention, unchanged.
pub type ToolArgs = HashMap<String, SandboxValue>;

/// A JS function (`function` or `async function`) exposed as a host tool.
///
/// `CalleeHandled = false`: the interpreter surfaces a failing tool as a
/// `ToolError`, which becomes a catchable `Exception` inside the script — so a
/// throwing tool is a normal outcome we handle, not a Node-style error-first
/// callback.
pub type JsToolFunction = ThreadsafeFunction<
    ToolArgs,
    Either<Promise<SandboxValue>, SandboxValue>,
    ToolArgs,
    napi::Status,
    false,
>;

pub struct JsToolHandler {
    func: JsToolFunction,
}

impl JsToolHandler {
    pub const fn new(func: JsToolFunction) -> Self {
        Self { func }
    }
}

/// One entry of the `tools` object: either a bare function, or `{ func,
/// parallelizable }`.
///
/// `FromNapiValue` is hand-written rather than derived via `#[napi(object)]`
/// because that would also demand `ToNapiValue`, and a `ThreadsafeFunction`
/// cannot be handed *back* to JavaScript. We only ever read tools in.
///
/// Crucially this type is **owned and `Send`**: a `ThreadsafeFunction` is, and a
/// `bool` is. That is what lets `execute` be an `async fn` at all — napi requires
/// its future to be `Send`, so no borrowed JS handle (`Object<'_>`, `Unknown<'_>`)
/// may survive into the async body. Everything is converted on the JS thread
/// first.
pub struct ToolEntry {
    pub func: JsToolFunction,
    pub parallelizable: bool,
}

impl TypeName for ToolEntry {
    fn type_name() -> &'static str {
        "Tool"
    }

    fn value_type() -> ValueType {
        ValueType::Unknown
    }
}

impl ValidateNapiValue for ToolEntry {
    unsafe fn validate(
        _env: napi::sys::napi_env,
        _napi_val: napi::sys::napi_value,
    ) -> NapiResult<napi::sys::napi_value> {
        // Both shapes are accepted; `from_napi_value` produces the precise error.
        Ok(std::ptr::null_mut())
    }
}

impl FromNapiValue for ToolEntry {
    unsafe fn from_napi_value(
        raw_env: napi::sys::napi_env,
        napi_val: napi::sys::napi_value,
    ) -> NapiResult<Self> {
        let env = Env::from_raw(raw_env);
        let unknown = unsafe { Unknown::from_napi_value(raw_env, napi_val)? };
        Self::from_js(&env, unknown)
    }
}

/// Wrap a JS tool so that it *always* returns a promise.
///
/// This is not cosmetic. napi's threadsafe function, in the callee-not-handled
/// mode, sends the return value down a `oneshot::channel::<Return>` — a channel
/// with no room for an error. So when a **synchronous** tool throws, the
/// exception cannot be handed back to the awaiting Rust future, and napi raises a
/// *fatal* exception instead: the throw escapes the sandbox boundary and takes
/// the process with it. The alternative mode (`CalleeHandled = true`) does
/// channel a `Result`, but only by imposing Node's error-first `(err, args)`
/// signature on every tool — an unacceptable thing to ask of a tool author.
///
/// An `async` wrapper resolves this at the source: a synchronous `throw` inside
/// an async function becomes a *rejected promise*, which travels the `Promise`
/// arm of the return type and arrives as a normal `Err`. The interpreter then
/// does what it should with a failing tool — surface a catchable `Exception`
/// inside the script.
///
/// The wrapper is compiled once per interpreter, not once per call.
fn async_shim(env: &Env) -> NapiResult<Function<'_, Unknown<'_>, Unknown<'_>>> {
    env.run_script("(fn) => async (args) => fn(args)")
}

impl ToolEntry {
    /// Read one entry of the `tools` object, wrapping its function so a
    /// synchronous throw cannot escape as a fatal exception.
    pub fn from_js(env: &Env, entry: Unknown<'_>) -> NapiResult<Self> {
        let (raw_func, parallelizable) = if entry.get_type()? == ValueType::Function {
            (entry, false)
        } else {
            let object = entry.coerce_to_object().map_err(|_| {
                Error::new(
                    Status::InvalidArg,
                    "a tool must be a function, or an object with a `func`".to_string(),
                )
            })?;

            let func: Unknown = object.get_named_property("func").map_err(|_| {
                Error::new(
                    Status::InvalidArg,
                    "a tool object must have a `func` property holding a function".to_string(),
                )
            })?;
            if func.get_type()? != ValueType::Function {
                return Err(Error::new(
                    Status::InvalidArg,
                    "a tool's `func` must be a function".to_string(),
                ));
            }

            let parallelizable: Option<bool> = object.get_named_property("parallelizable")?;
            (func, parallelizable.unwrap_or(false))
        };

        let wrapped = async_shim(env)?.call(raw_func)?;
        let func = JsToolFunction::from_unknown(wrapped)?;

        Ok(Self { func, parallelizable })
    }
}

#[async_trait]
impl ToolHandler for JsToolHandler {
    async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        let args: ToolArgs = kwargs.into_iter().map(|(k, v)| (k, SandboxValue(v))).collect();

        // Marshals onto the JS thread, invokes the function there (where the
        // conversion of its return value also happens, since napi values are
        // thread-bound), and comes back with an owned result.
        let returned =
            self.func.call_async(args).await.map_err(|e| ToolError::new(e.reason.clone()))?;

        match returned {
            // `async function`, or any function returning a thenable.
            Either::A(promise) => {
                promise.await.map(|value| value.0).map_err(|e| ToolError::new(e.reason.clone()))
            }
            // Plain `function`.
            Either::B(value) => Ok(value.0),
        }
    }
}
