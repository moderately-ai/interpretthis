// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `hashlib` module.
//!
//! Supports md5 / sha1 / sha256 / sha512 via the RustCrypto crates,
//! returning a `Value::HashDigest` that carries the algorithm name and the
//! accumulated *input* bytes. The digest is computed lazily on
//! `.hexdigest()` / `.digest()`, so the CPython create-then-`update` pattern
//! (`h = sha256(); h.update(a); h.update(b); h.hexdigest()`) works — each
//! `update` appends to the buffer.

use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    value::Value,
};

pub fn has_function(name: &str) -> bool {
    matches!(name, "md5" | "sha1" | "sha256" | "sha512")
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    // The optional argument seeds the hash buffer; no argument starts empty.
    let input = match args.first() {
        None => Vec::new(),
        Some(_) => arg_bytes(func, args)?,
    };
    if !matches!(func, "md5" | "sha1" | "sha256" | "sha512") {
        return Err(InterpreterError::AttributeError(format!(
            "module 'hashlib' has no attribute '{func}'"
        ))
        .into());
    }
    Ok(Value::HashDigest { algo: func.to_string(), bytes: input })
}

/// Compute the digest of `input` under `algo` (the accumulated buffer).
fn compute_digest(algo: &str, input: &[u8]) -> Vec<u8> {
    match algo {
        "md5" => Md5::digest(input).to_vec(),
        "sha1" => Sha1::digest(input).to_vec(),
        "sha256" => Sha256::digest(input).to_vec(),
        // Any unknown algo cannot be constructed (call() gates it), so sha512
        // is the only remaining case.
        _ => Sha512::digest(input).to_vec(),
    }
}

/// Byte lengths of each digest and its internal block, for `.digest_size` /
/// `.block_size`.
fn sizes(algo: &str) -> (i64, i64) {
    match algo {
        "md5" => (16, 64),
        "sha1" => (20, 64),
        "sha256" => (32, 64),
        _ => (64, 128),
    }
}

/// Dispatch a method on a `HashDigest` value. `input` is the accumulated
/// buffer; the digest is computed here rather than stored.
pub fn dispatch_hash_method(
    algo: &str,
    input: &[u8],
    method: &str,
    _args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "hexdigest" => Ok(Value::String(hex::encode(compute_digest(algo, input)).into())),
        "digest" => Ok(Value::Bytes(compute_digest(algo, input))),
        "name" => Ok(Value::String(algo.into())),
        "digest_size" => Ok(Value::Int(sizes(algo).0)),
        "block_size" => Ok(Value::Int(sizes(algo).1)),
        _ => Err(InterpreterError::AttributeError(format!(
            "'{algo} HASH' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Attribute (not method) access on a `HashDigest`: `.name`, `.digest_size`,
/// `.block_size`.
pub fn hash_attribute(algo: &str, _input: &[u8], name: &str) -> EvalResult {
    match name {
        "name" => Ok(Value::String(algo.into())),
        "digest_size" => Ok(Value::Int(sizes(algo).0)),
        "block_size" => Ok(Value::Int(sizes(algo).1)),
        _ => Err(InterpreterError::AttributeError(format!(
            "'{algo} HASH' object has no attribute '{name}'"
        ))
        .into()),
    }
}

fn arg_bytes(func: &str, args: &[Value]) -> Result<Vec<u8>, EvalError> {
    match args.first() {
        Some(Value::Bytes(b)) => Ok(b.clone()),
        Some(Value::ByteArray(b)) => Ok(b.lock().clone()),
        _ => Err(InterpreterError::TypeError(format!(
            "{func}() argument must be a bytes-like object"
        ))
        .into()),
    }
}

pub struct HashlibModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for HashlibModule {
    fn name(&self) -> &'static str {
        "hashlib"
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
