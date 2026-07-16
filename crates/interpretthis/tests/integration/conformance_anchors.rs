// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gate: every user-visible "unsupported feature" error must point at a
//! CONFORMANCE.md anchor so a caller who hits one can find the documented
//! limitation. These run the interpreter on each unsupported construct and
//! assert the raised error carries `CONFORMANCE.md#`.

use std::collections::HashMap;

use interpretthis::{Interpreter, InterpreterConfig, InterpreterDeps, Tools};

/// Run `code` and return the debug-rendered error (panics if it didn't raise).
async fn error_text(code: &str) -> String {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let resp = interp.execute(code, &Tools::new(), HashMap::new()).await;
    let err = resp
        .error
        .unwrap_or_else(|| panic!("code should have raised an unsupported-feature error:\n{code}"));
    format!("{err:?}")
}

/// Assert `code` raises an error whose message references a CONFORMANCE anchor.
async fn assert_has_anchor(code: &str) {
    let text = error_text(code).await;
    assert!(
        text.contains("CONFORMANCE.md#"),
        "unsupported-feature error is missing a CONFORMANCE anchor:\n  code: {code}\n  error: {text}"
    );
}

// `async def` / `await` / `asyncio` and `async for` / `async with` over user
// async iterators / context managers are now supported (sequential coroutines);
// async generators remain unsupported but fail with a runtime error rather than
// a dedicated "unsupported feature" anchor, so they are not gated here.
