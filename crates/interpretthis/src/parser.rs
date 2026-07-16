// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use rustpython_parser::{
    Parse,
    ast::{self, Suite},
};

use crate::error::InterpreterError;

/// Parse Python source code into an AST suite (list of statements).
///
/// Applies CPython's compile-time private-name mangling (`__x` inside a class
/// body → `_ClassName__x`) as a post-parse AST pass.
pub fn parse(code: &str) -> Result<Suite, InterpreterError> {
    let suite = ast::Suite::parse(code, "<interpreter>")
        .map_err(|e| InterpreterError::Syntax(format!("{e}")))?;
    Ok(crate::mangle::mangle_private_names(suite))
}
