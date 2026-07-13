// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `hashlib` module.
//!
//! Supports sha256 / sha512 via the workspace `sha2` crate, returning
//! a `Value::HashDigest` carrying the algorithm name and digest bytes.
//! Methods `.hexdigest()` and `.digest()` round-trip the bytes.
//!
//! The CPython API is "create, optional update, digest". Our shim is
//! immediate: `hashlib.sha256(data)` runs the digest in one shot.
//! The create-then-update pattern (`h = sha256(); h.update(...);
//! h.digest()`) is not modelled because eager digesting matches every
//! observed extraction-script use case.
//!
//! md5 / sha1 are not modelled because they are not in the workspace
//! crate set and are increasingly inappropriate for hashing anything
//! other than a content-addressable cache key.

use sha2::{Digest, Sha256, Sha512};

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    value::Value,
};

pub fn has_function(name: &str) -> bool {
    matches!(name, "sha256" | "sha512")
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        "sha256" => {
            let input = arg_bytes(func, args)?;
            let bytes = Sha256::digest(&input).to_vec();
            Ok(Value::HashDigest { algo: "sha256".to_string(), bytes })
        }
        "sha512" => {
            let input = arg_bytes(func, args)?;
            let bytes = Sha512::digest(&input).to_vec();
            Ok(Value::HashDigest { algo: "sha512".to_string(), bytes })
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'hashlib' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// Dispatch a method on a `HashDigest` value.
pub fn dispatch_hash_method(
    algo: &str,
    bytes: &[u8],
    method: &str,
    _args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "hexdigest" => Ok(Value::String(hex::encode(bytes).into())),
        "digest" => Ok(Value::Bytes(bytes.to_vec())),
        "digest_size" | "block_size" | "name" => match method {
            "name" => Ok(Value::String(algo.into())),
            // digest_size: 32 for sha256, 64 for sha512.
            "digest_size" => {
                #[expect(
                    clippy::cast_possible_wrap,
                    reason = "hash output lengths are 32 (sha256) or 64 (sha512) bytes — bounded by the sha2 crate's fixed-size GenericArray, well below i64::MAX"
                )]
                let size = bytes.len() as i64;
                Ok(Value::Int(size))
            }
            // block_size: 64 for sha256, 128 for sha512.
            "block_size" => Ok(Value::Int(match algo {
                "sha256" => 64,
                "sha512" => 128,
                _ => 0,
            })),
            _ => unreachable!(),
        },
        _ => Err(InterpreterError::AttributeError(format!(
            "'{algo} HASH' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Read a hash-object attribute. CPython exposes `.name`,
/// `.digest_size`, `.block_size` as attributes (not methods); we
/// support both so user code that uses either form works.
pub fn hash_attribute(algo: &str, bytes: &[u8], attr: &str) -> EvalResult {
    match attr {
        "name" => Ok(Value::String(algo.into())),
        "digest_size" => {
            #[expect(
                clippy::cast_possible_wrap,
                reason = "hash output lengths are 32 (sha256) or 64 (sha512) bytes — bounded by the sha2 crate's fixed-size GenericArray, well below i64::MAX"
            )]
            let size = bytes.len() as i64;
            Ok(Value::Int(size))
        }
        "block_size" => Ok(Value::Int(match algo {
            "sha256" => 64,
            "sha512" => 128,
            _ => 0,
        })),
        _ => Err(InterpreterError::AttributeError(format!(
            "'{algo} HASH' object has no attribute '{attr}'"
        ))
        .into()),
    }
}

/// Coerce the hash-input argument to bytes. CPython requires `bytes`
/// or `bytearray`; `str` raises TypeError (you must `.encode()`
/// first). We match that exactly so corpus snippets byte-diff against
/// python3.
fn arg_bytes(func: &str, args: &[Value]) -> Result<Vec<u8>, EvalError> {
    let value = args.first().ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!("{func}() missing required argument")))
    })?;
    match value {
        Value::Bytes(b) => Ok(b.clone()),
        other => Err(InterpreterError::TypeError(format!(
            "Strings must be encoded before hashing (got '{}')",
            other.type_name()
        ))
        .into()),
    }
}

/// `hashlib` module registration.
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
