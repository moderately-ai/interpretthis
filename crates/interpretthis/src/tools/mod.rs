// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Host tool injection surface for the interpreter.
//!
//! # Timeouts
//!
//! When [`crate::InterpreterConfig::max_execution_time`] is set, each tool
//! call receives the **remaining** wall-clock budget (not a fresh full
//! budget). Parallel tools share that same remaining budget via
//! `tokio::time::timeout` per task. A timeout becomes [`ToolError`], which
//! the eval layer maps to a catchable Python `Exception` if the call site
//! is inside user `try`/`except`, otherwise a host [`crate::InterpreterError::Tool`].

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;

use crate::value::Value;

/// Configuration for a tool provided by the host.
#[derive(Clone)]
pub struct ToolDefinition {
    /// The tool handler. Uses `Arc` to allow cloning into spawned tasks
    /// for parallelizable tools.
    pub handler: Arc<dyn ToolHandler>,
    /// Whether this tool is safe for parallel (deferred) execution.
    /// Default: false (volatile/sequential).
    pub parallelizable: bool,
}

impl ToolDefinition {
    /// Create a volatile (sequential) tool config.
    pub fn new(handler: impl ToolHandler + 'static) -> Self {
        Self { handler: Arc::new(handler), parallelizable: false }
    }

    /// Create a parallelizable tool config.
    pub fn parallel(handler: impl ToolHandler + 'static) -> Self {
        Self { handler: Arc::new(handler), parallelizable: true }
    }

    /// Create a volatile tool from an async closure.
    ///
    /// # Example
    /// ```ignore
    /// ToolDefinition::from_fn(|kwargs| async move {
    ///     let query = kwargs.require_str("query")?.to_string();
    ///     Ok(Value::String(format!("found: {query}")))
    /// })
    /// ```
    pub fn from_fn<F, Fut>(f: F) -> Self
    where
        F: Fn(HashMap<String, Value>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        Self { handler: Arc::new(FnTool(f)), parallelizable: false }
    }

    /// Create a parallelizable tool from an async closure.
    pub fn from_fn_parallel<F, Fut>(f: F) -> Self
    where
        F: Fn(HashMap<String, Value>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        Self { handler: Arc::new(FnTool(f)), parallelizable: true }
    }
}

/// Error returned by tool handlers.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct ToolError {
    pub message: String,
}

impl ToolError {
    /// Create a new tool error with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }

    /// Create a tool error from any `std::error::Error`.
    ///
    /// This enables using `?` with any error type in tool handlers:
    /// ```ignore
    /// async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
    ///     let data = reqwest::get("...").await.map_err(ToolError::from_err)?;
    ///     Ok(Value::String(data))
    /// }
    /// ```
    pub fn from_err(err: impl std::fmt::Display) -> Self {
        Self { message: err.to_string() }
    }
}

/// Async trait for host-provided tools.
///
/// Tools receive keyword arguments as a `HashMap<String, Value>`.
/// Positional arguments from Python are passed as `"arg0"`, `"arg1"`, etc.
/// Keyword arguments use their Python name (e.g., `search(query="text")`
/// passes `{"query": Value::String("text")}`).
///
/// Use the [`KwargsExt`] trait for convenient argument extraction.
#[async_trait]
pub trait ToolHandler: Send + Sync {
    /// Invoke the tool with keyword arguments from Python.
    ///
    /// Positional args arrive as `"arg0"`, `"arg1"`, …; keyword args keep
    /// their Python names. Prefer [`KwargsExt`] for extraction.
    ///
    /// # Errors
    ///
    /// Return [`ToolError`] for any failure. It becomes
    /// [`crate::InterpreterError::Tool`] for the host. Inside user Python
    /// it is raised as a generic `Exception` (catchable by bare `except`).
    async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError>;
}

/// A validated map of tool names to tool configurations.
///
/// Use `Tools::new()` to create an empty set, then `.with()` for fluent
/// chaining or `.insert()` for mutable construction.
///
/// Tool names are validated at insertion time — dangerous names
/// (e.g., `eval`, `exec`, `os`) are rejected immediately.
///
/// # Example
/// ```ignore
/// let tools = Tools::new()
///     .with("search", ToolDefinition::from_fn(|kwargs| async move {
///         let q = kwargs.require_str("query")?;
///         Ok(Value::String(format!("found: {q}")))
///     }))
///     .with("fetch", ToolDefinition::parallel(FetchTool));
/// ```
#[derive(Clone)]
pub struct Tools(HashMap<String, ToolDefinition>);

impl Tools {
    /// Create an empty tool set.
    #[must_use]
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Insert a tool and return `self` for chaining. Validates the name
    /// immediately (no deferred `.build()`).
    ///
    /// # Panics
    ///
    /// Panics if the tool name is a dangerous builtin (e.g. `eval`, `exec`).
    #[must_use]
    pub fn with(mut self, name: &str, config: ToolDefinition) -> Self {
        self.insert(name, config);
        self
    }

    /// Insert a tool into the set.
    ///
    /// # Panics
    /// Panics if the tool name is a dangerous builtin.
    pub fn insert(&mut self, name: &str, config: ToolDefinition) {
        assert!(
            crate::security::validator::is_name_allowed(name),
            "tool name '{name}' is a dangerous builtin and cannot be registered"
        );
        self.0.insert(name.to_string(), config);
    }

    /// Look up a tool by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.0.get(name)
    }

    /// Check if a tool with the given name exists.
    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// Iterate over tool names.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    /// Check if the tool set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Merge another tool set into this one. The `other` tools take priority
    /// on name conflicts.
    #[must_use]
    pub fn merged_with(&self, other: &Self) -> Self {
        let mut merged = self.0.clone();
        for (name, config) in &other.0 {
            merged.insert(name.clone(), config.clone());
        }
        Self(merged)
    }
}

impl Default for Tools {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Kwargs extraction helpers
// ---------------------------------------------------------------------------

/// Extension trait for convenient argument extraction from tool kwargs.
pub trait KwargsExt {
    /// Get a required string argument, or error.
    ///
    /// # Errors
    /// Returns a `ToolError` if the key is missing or not a string value.
    fn require_str(&self, key: &str) -> Result<&str, ToolError>;
    /// Get a required integer argument, or error.
    ///
    /// # Errors
    /// Returns a `ToolError` if the key is missing or not an integer value.
    fn require_int(&self, key: &str) -> Result<i64, ToolError>;
    /// Get a required float argument, or error.
    ///
    /// # Errors
    /// Returns a `ToolError` if the key is missing or not a float value.
    fn require_float(&self, key: &str) -> Result<f64, ToolError>;
    /// Get a required bool argument, or error.
    ///
    /// # Errors
    /// Returns a `ToolError` if the key is missing or not a bool value.
    fn require_bool(&self, key: &str) -> Result<bool, ToolError>;
    /// Get an optional string argument.
    fn get_str(&self, key: &str) -> Option<&str>;
    /// Get an optional integer argument.
    fn get_int(&self, key: &str) -> Option<i64>;
    /// Get an optional float argument.
    fn get_float(&self, key: &str) -> Option<f64>;
    /// Get an optional bool argument.
    fn get_bool(&self, key: &str) -> Option<bool>;
    /// Get a value by key, returning a default if missing.
    fn get_or(&self, key: &str, default: Value) -> Value;
}

impl<S: std::hash::BuildHasher> KwargsExt for HashMap<String, Value, S> {
    fn require_str(&self, key: &str) -> Result<&str, ToolError> {
        self.get(key)
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new(format!("missing required string argument '{key}'")))
    }

    fn require_int(&self, key: &str) -> Result<i64, ToolError> {
        self.get(key)
            .and_then(super::value::Value::as_int)
            .ok_or_else(|| ToolError::new(format!("missing required integer argument '{key}'")))
    }

    fn require_float(&self, key: &str) -> Result<f64, ToolError> {
        self.get(key)
            .and_then(super::value::Value::as_float)
            .ok_or_else(|| ToolError::new(format!("missing required float argument '{key}'")))
    }

    fn require_bool(&self, key: &str) -> Result<bool, ToolError> {
        self.get(key)
            .and_then(super::value::Value::as_bool)
            .ok_or_else(|| ToolError::new(format!("missing required bool argument '{key}'")))
    }

    fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.as_str())
    }

    fn get_int(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(super::value::Value::as_int)
    }

    fn get_float(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(super::value::Value::as_float)
    }

    fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(super::value::Value::as_bool)
    }

    fn get_or(&self, key: &str, default: Value) -> Value {
        self.get(key).cloned().unwrap_or(default)
    }
}

// ---------------------------------------------------------------------------
// FnTool — wraps a closure as a ToolHandler
// ---------------------------------------------------------------------------

struct FnTool<F>(F);

#[async_trait]
impl<F, Fut> ToolHandler for FnTool<F>
where
    F: Fn(HashMap<String, Value>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Value, ToolError>> + Send + 'static,
{
    async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        (self.0)(kwargs).await
    }
}

pub(crate) mod lazy_proxy;
pub(crate) mod resolver;
