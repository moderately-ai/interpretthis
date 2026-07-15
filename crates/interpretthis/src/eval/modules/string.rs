// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `string` module.
//!
//! Exposes the standard character-class constants, the `Template` class
//! constructor, and `capwords`.

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
        matches!(name, "Template" | "capwords")
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
            // `string.capwords(s, sep=None)` — split on `sep` (whitespace when
            // None), capitalize each word, and rejoin with `sep` (a single
            // space when None). Equivalent to
            // `(sep or ' ').join(w.capitalize() for w in s.split(sep))`.
            "capwords" => {
                let Some(Value::String(s)) = args.first() else {
                    return Err(InterpreterError::TypeError(
                        "capwords() requires a string argument".into(),
                    )
                    .into());
                };
                let sep = match args.get(1) {
                    None | Some(Value::None) => None,
                    Some(Value::String(sp)) => Some(sp.as_str()),
                    Some(other) => {
                        return Err(InterpreterError::TypeError(format!(
                            "capwords() sep must be str or None, not {}",
                            other.type_name()
                        ))
                        .into());
                    }
                };
                let words: Vec<String> = match sep {
                    None => s.split_whitespace().map(py_capitalize).collect(),
                    Some("") => {
                        return Err(InterpreterError::ValueError("empty separator".into()).into());
                    }
                    Some(sp) => s.split(sp).map(py_capitalize).collect(),
                };
                Ok(Value::String(words.join(sep.unwrap_or(" ")).into()))
            }
            _ => Err(InterpreterError::AttributeError(format!(
                "module 'string' has no callable '{func}'"
            ))
            .into()),
        }
    }
}

/// Python `str.capitalize`: first character upper-cased, the rest lower-cased.
fn py_capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect(),
    }
}
