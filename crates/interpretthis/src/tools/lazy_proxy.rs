// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{tools::ToolError, value::Value};

/// A deferred tool call result. Wraps a tokio `JoinHandle`.
/// Resolves lazily when the value is consumed.
#[derive(Clone)]
pub struct LazyProxy {
    inner: Arc<Mutex<LazyProxyInner>>,
    /// The name of the tool this proxy wraps.
    pub tool_name: String,
}

struct LazyProxyInner {
    handle: Option<tokio::task::JoinHandle<Result<Value, ToolError>>>,
    resolved: Option<Result<Value, ToolError>>,
}

impl LazyProxy {
    /// Create a new lazy proxy wrapping a spawned task.
    pub fn new(
        handle: tokio::task::JoinHandle<Result<Value, ToolError>>,
        tool_name: String,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LazyProxyInner { handle: Some(handle), resolved: None })),
            tool_name,
        }
    }

    /// Resolve the proxy — await the task result. Idempotent.
    pub async fn resolve(&self) -> Result<Value, ToolError> {
        let mut inner = self.inner.lock().await;

        // Return cached result if already resolved
        if let Some(ref result) = inner.resolved {
            return result.clone();
        }

        // Await the handle
        let Some(handle) = inner.handle.take() else {
            return Err(ToolError::new("LazyProxy handle already consumed"));
        };
        let result = match handle.await {
            Ok(r) => r,
            Err(e) => Err(ToolError::new(format!("task join error: {e}"))),
        };
        inner.resolved = Some(result.clone());
        result
    }

    /// Check if this proxy has been resolved yet.
    pub async fn is_resolved(&self) -> bool {
        self.inner.lock().await.resolved.is_some()
    }
}

impl std::fmt::Debug for LazyProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<LazyProxy tool={}>", self.tool_name)
    }
}
