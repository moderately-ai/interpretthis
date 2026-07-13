// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use rustpython_parser::{
    Parse,
    ast::{self, Suite},
};

use crate::error::InterpreterError;

/// Parse Python source code into an AST suite (list of statements).
pub fn parse(code: &str) -> Result<Suite, InterpreterError> {
    ast::Suite::parse(code, "<interpreter>").map_err(|e| InterpreterError::Syntax(format!("{e}")))
}
