// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `string` module — constants only.
//!
//! Exposes the standard character-class constants. `string.Template`
//! is not modelled (string interpolation in this interpreter uses
//! f-strings and `.format()`, which cover every observed extraction-
//! script case).

use crate::value::Value;

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

/// `string` module registration. Constants-only; the default
/// [`Module::call`] returns "no callable" for any invocation, and
/// [`Module::has_function`] defaults to false.
pub struct StringModule;

impl crate::eval::modules::Module for StringModule {
    fn name(&self) -> &'static str {
        "string"
    }
    fn constant(&self, name: &str) -> Option<Value> {
        constant(name)
    }
}
