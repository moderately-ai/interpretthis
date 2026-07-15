// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `base64` module.
//!
//! Supports b64encode / b64decode and their urlsafe variants. Both
//! accept and return bytes — the CPython API. Encoding fails on
//! non-bytes input; decoding raises a clear ValueError on malformed
//! base64.

use base64::Engine as _;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::modules::value_error,
    value::Value,
};

pub fn has_function(name: &str) -> bool {
    matches!(
        name,
        "b64encode"
            | "b64decode"
            | "urlsafe_b64encode"
            | "urlsafe_b64decode"
            | "b16encode"
            | "b16decode"
            | "b32encode"
            | "b32decode"
    )
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    let input = arg_bytes(func, args)?;
    let result = match func {
        "b64encode" => base64::engine::general_purpose::STANDARD.encode(&input).into_bytes(),
        "urlsafe_b64encode" => {
            base64::engine::general_purpose::URL_SAFE.encode(&input).into_bytes()
        }
        "b64decode" => base64::engine::general_purpose::STANDARD
            .decode(&input)
            .map_err(|e| value_error(format!("Invalid base64-encoded string: {e}")))?,
        "urlsafe_b64decode" => base64::engine::general_purpose::URL_SAFE
            .decode(&input)
            .map_err(|e| value_error(format!("Invalid base64-encoded string: {e}")))?,
        // Base16 is uppercase hex (CPython b16encode); b16decode accepts the
        // uppercase form CPython produces (and, leniently, lowercase).
        "b16encode" => hex::encode_upper(&input).into_bytes(),
        "b16decode" => hex::decode(String::from_utf8_lossy(&input).to_uppercase())
            .map_err(|e| value_error(format!("Non-base16 digit found: {e}")))?,
        "b32encode" => b32encode(&input).into_bytes(),
        "b32decode" => b32decode(&String::from_utf8_lossy(&input))?,
        _ => {
            return Err(InterpreterError::AttributeError(format!(
                "module 'base64' has no attribute '{func}'"
            ))
            .into());
        }
    };
    Ok(Value::Bytes(result))
}

const B32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// RFC 4648 base32 encode: 5-byte groups → 8 chars, `=`-padded.
fn b32encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(5) * 8);
    for chunk in input.chunks(5) {
        let mut buf = [0u8; 5];
        buf[..chunk.len()].copy_from_slice(chunk);
        // Pack the (up to) 40 bits, then emit 8 5-bit groups MSB-first.
        let bits = (u64::from(buf[0]) << 32)
            | (u64::from(buf[1]) << 24)
            | (u64::from(buf[2]) << 16)
            | (u64::from(buf[3]) << 8)
            | u64::from(buf[4]);
        // Number of output chars that carry data for a partial final chunk.
        let out_chars = match chunk.len() {
            1 => 2,
            2 => 4,
            3 => 5,
            4 => 7,
            _ => 8,
        };
        for i in 0..8 {
            if i < out_chars {
                let idx = ((bits >> (35 - i * 5)) & 0x1f) as usize;
                out.push(B32_ALPHABET[idx] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// RFC 4648 base32 decode (uppercase, `=`-padded).
fn b32decode(input: &str) -> Result<Vec<u8>, EvalError> {
    let trimmed: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(trimmed.len() * 5 / 8);
    let mut bits: u64 = 0;
    let mut nbits = 0u32;
    for &c in &trimmed {
        let val = B32_ALPHABET
            .iter()
            .position(|&a| a == c)
            .ok_or_else(|| value_error("Non-base32 digit found".to_string()))?;
        bits = (bits << 5) | val as u64;
        nbits += 5;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    Ok(out)
}

fn arg_bytes(func: &str, args: &[Value]) -> Result<Vec<u8>, EvalError> {
    let value = args.first().ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!("{func}() missing required argument")))
    })?;
    match value {
        Value::Bytes(b) => Ok(b.clone()),
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(InterpreterError::TypeError(format!(
            "{func}() requires bytes or str (got '{}')",
            other.type_name()
        ))
        .into()),
    }
}

/// `base64` module registration.
pub struct Base64Module;

#[async_trait::async_trait]
impl crate::eval::modules::Module for Base64Module {
    fn name(&self) -> &'static str {
        "base64"
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        _state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &indexmap::IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        call(func, args)
    }
}
