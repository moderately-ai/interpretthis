// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `string` module — constants only.
//!
//! Exposes the standard character-class constants. `string.Template`
//! is not modelled (string interpolation in this interpreter uses
//! f-strings and `.format()`, which cover every observed extraction-
//! script case).

use crate::{
    error::{EvalResult, InterpreterError},
    value::Value,
};

pub fn constant(name: &str) -> Option<Value> {
    let s = match name {
        "ascii_lowercase" => "abcdefghijklmnopqrstuvwxyz",
        "ascii_uppercase" => "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "ascii_letters" => "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "digits" => "0123456789",
        "hexdigits" => "0123456789abcdefABCDEF",
        "octdigits" => "01234567",
        "punctuation" => "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~",
        "whitespace" => " \t\n\r\x0b\x0c",
        "printable" => {
            "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~ \t\n\r\x0b\x0c"
        }
        _ => return None,
    };
    Some(Value::String(s.into()))
}

/// `string` module registration. Exposes the character-class constants
/// plus the `Template` class constructor.
pub struct StringModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for StringModule {
    fn name(&self) -> &'static str {
        "string"
    }
    fn constant(&self, name: &str) -> Option<Value> {
        constant(name)
    }
    fn has_function(&self, name: &str) -> bool {
        name == "Template"
    }
    async fn call(
        &self,
        _state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &indexmap::IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        match func {
            // `string.Template(template_string)`.
            "Template" => match args.first() {
                Some(Value::String(s)) => Ok(Value::Template(s.clone())),
                Some(other) => Err(InterpreterError::TypeError(format!(
                    "Template() argument must be str, not {}",
                    other.type_name()
                ))
                .into()),
                None => Err(InterpreterError::TypeError(
                    "Template() missing required argument: 'template'".into(),
                )
                .into()),
            },
            _ => Err(InterpreterError::AttributeError(format!(
                "module 'string' has no callable '{func}'"
            ))
            .into()),
        }
    }
}
