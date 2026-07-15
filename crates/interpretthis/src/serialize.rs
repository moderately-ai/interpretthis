// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    error::InterpreterError,
    state::{InterpreterState, estimate_value_size},
    value::{ClassValue, Value},
};

/// Wire-format version embedded in every exported state blob.
///
/// Bump whenever a backwards-incompatible change lands in any type that
/// participates in state serde (variables, classes, function sources, …).
/// Mismatched blobs fail with
/// [`crate::InterpreterError::StateFormatSuperseded`] rather than silent
/// mis-deserialization.
///
/// Pre-versioning blobs (raw JSON without the 4-byte prefix) are
/// rejected as `found = 0`.
///
/// Version 2: dict keys fold `Value::Bool` into `ValueKey::Int(0|1)` so
/// `{True: x}[1]` matches CPython bool-is-int semantics.
///
/// Version 3: `ClassValue` gains `bases` / `mro` (C3 linearisation).
///
/// Version 4: `ClassValue` gains property / staticmethod / classmethod maps.
///
/// Later versions add `Counter`, datetime variants, `HashDigest`,
/// `Deque` / `DefaultDict`, `EnumMember`, dataclass metadata, and
/// `Decimal` / `Fraction` — each bump rejects older readers of newer
/// blobs (one-directional incompatibility). v12: `set`/`frozenset` moved to a
/// shared CPython-order table, serialized as elements-only.
pub const STATE_FORMAT_VERSION: u32 = 12;

/// Bytes occupied by the little-endian `u32` version prefix before the
/// JSON state body.
const VERSION_PREFIX_SIZE: usize = 4;

/// The serializable representation of interpreter state.
#[derive(Serialize, Deserialize)]
struct SerializedState {
    /// User variables. `Value::Function` rides through naturally — the
    /// function body source is carried on the `FunctionDef` struct itself,
    /// so there is no separate `function_sources` payload.
    variables: BTreeMap<String, Value>,
    /// User-defined classes. Like functions, each method carries its own
    /// source, so its body AST is rebuilt on import; instances hold only a
    /// class name and resolve methods against this registry.
    #[serde(default)]
    classes: BTreeMap<String, ClassValue>,
}

/// Maximum serialized state size we'll accept on import (16MB).
const MAX_IMPORT_SIZE: usize = 16 * 1024 * 1024;

/// Export interpreter state to bytes.
///
/// Wire layout:
/// ```text
/// [0..4)   STATE_FORMAT_VERSION as little-endian u32
/// [4..]    JSON-encoded state body
/// ```
///
/// Hosts may wrap the blob (HMAC, compression, …); version is checked
/// on import after those layers are peeled off.
pub fn export_state(state: &InterpreterState) -> Result<Vec<u8>, InterpreterError> {
    let mut variables = BTreeMap::new();
    for (key, value) in &state.variables {
        // Skip internal keys
        if key.starts_with('_') {
            continue;
        }
        // Lambda bodies are stored AST-side (keyed by
        // LambdaDef::lambda_id). The source string carried on the
        // LambdaDef is re-parsed on import to rebuild lambda_bodies
        // -- same pattern as FunctionDef. So we DO serialize
        // lambdas; the cache reconstruction is import-side.
        // LazyProxy wraps a runtime future; nothing meaningful to serialize.
        if matches!(value, Value::LazyProxy(_)) {
            continue;
        }
        variables.insert(key.clone(), value.clone());
    }

    let classes = state.classes.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let serialized = SerializedState { variables, classes };

    let body = serde_json::to_vec(&serialized)
        .map_err(|e| InterpreterError::Runtime(format!("failed to serialize state: {e}")))?;

    let mut out = Vec::with_capacity(VERSION_PREFIX_SIZE + body.len());
    out.extend_from_slice(&STATE_FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&body);

    debug_assert!(out.len() >= VERSION_PREFIX_SIZE, "exported state always carries version prefix");
    debug_assert!(
        out.len() == VERSION_PREFIX_SIZE + body.len(),
        "exported state is version prefix + json body, nothing else"
    );
    Ok(out)
}

/// Import interpreter state from bytes.
///
/// Replaces all current variables with the imported state. For every
/// restored `Value::Function`, the function source string (carried on the
/// struct) is re-parsed to repopulate the `function_bodies` parse cache
/// and is validated the same way a fresh `def` would be (single
/// `FunctionDef` statement with a name matching the variable key, name
/// passing the security allowlist). Memory tracking is recomputed from
/// scratch after import.
pub fn import_state(state: &mut InterpreterState, data: &[u8]) -> Result<(), InterpreterError> {
    // Size limit check before deserialization
    if data.len() > MAX_IMPORT_SIZE {
        return Err(InterpreterError::LimitExceeded(format!(
            "serialized state ({} bytes) exceeds import limit ({MAX_IMPORT_SIZE} bytes)",
            data.len()
        )));
    }

    // Read the 4-byte little-endian version prefix. Blobs shorter than
    // the prefix, or with a prefix that doesn't match the current
    // STATE_FORMAT_VERSION (including pre-versioning JSON blobs, which
    // start with `{` and decode as a nonsense version number), are
    // rejected as superseded. There is intentionally no compatibility
    // path — silent migration across format versions is the failure
    // mode this gate exists to prevent.
    let Some(version_bytes) = data.get(..VERSION_PREFIX_SIZE) else {
        return Err(InterpreterError::StateFormatSuperseded {
            found: 0,
            expected: STATE_FORMAT_VERSION,
        });
    };
    // The slice length is the const VERSION_PREFIX_SIZE so try_into is
    // infallible; map the impossible Err onto the same superseded error
    // to keep the function `?`-free.
    let Ok(version_array) = <[u8; VERSION_PREFIX_SIZE]>::try_from(version_bytes) else {
        return Err(InterpreterError::StateFormatSuperseded {
            found: 0,
            expected: STATE_FORMAT_VERSION,
        });
    };
    let found_version = u32::from_le_bytes(version_array);
    if found_version != STATE_FORMAT_VERSION {
        return Err(InterpreterError::StateFormatSuperseded {
            found: found_version,
            expected: STATE_FORMAT_VERSION,
        });
    }
    let body = &data[VERSION_PREFIX_SIZE..];

    let serialized: SerializedState = serde_json::from_slice(body)
        .map_err(|e| InterpreterError::Runtime(format!("failed to deserialize state: {e}")))?;

    // Validate imported variable names — reject dangerous names.
    for key in serialized.variables.keys() {
        if !crate::security::validator::is_name_allowed(key) {
            return Err(InterpreterError::Security(format!(
                "imported state contains dangerous variable name '{key}'"
            )));
        }
    }

    // Validate and rebuild parse caches for any restored functions before
    // they are committed to interpreter state, so a malformed function
    // source aborts the import with the interpreter untouched.
    let mut restored_bodies: Vec<(String, Vec<rustpython_parser::ast::Stmt>)> = Vec::new();
    for (name, value) in &serialized.variables {
        let Value::Function(func) = value else {
            continue;
        };

        // Parse the function source code.
        let stmts = crate::parser::parse(&func.source).map_err(|e| {
            InterpreterError::Runtime(format!("failed to re-parse function '{name}': {e}"))
        })?;

        // Validate: must contain exactly one FunctionDef statement.
        if stmts.len() != 1 {
            return Err(InterpreterError::Security(format!(
                "imported function '{name}' source contains {} statements (expected 1)",
                stmts.len()
            )));
        }

        match stmts.into_iter().next() {
            Some(rustpython_parser::ast::Stmt::FunctionDef(func_node)) => {
                // The stored function name must match the declared name;
                // diverging would mean the variable key was forged after
                // serialization.
                if func_node.name.as_str() != name {
                    return Err(InterpreterError::Security(format!(
                        "imported function source declares '{}' but key is '{name}'",
                        func_node.name
                    )));
                }
                if func.name != *name {
                    return Err(InterpreterError::Security(format!(
                        "imported function '{name}' carries a mismatched inner name '{}'",
                        func.name
                    )));
                }
                restored_bodies.push((name.clone(), func_node.body));
            }
            _ => {
                return Err(InterpreterError::Security(format!(
                    "imported function '{name}' source is not a function definition"
                )));
            }
        }
    }

    // Rebuild method body ASTs for every restored class, keyed by the same
    // qualified `Class.method` name the definition path uses. Validated before
    // any state mutation so a malformed method source aborts cleanly.
    let mut restored_method_bodies: Vec<(String, Vec<rustpython_parser::ast::Stmt>)> = Vec::new();
    for class in serialized.classes.values() {
        for method in class.methods.values() {
            let stmts = crate::parser::parse(&method.source).map_err(|e| {
                InterpreterError::Runtime(format!(
                    "failed to re-parse method '{}': {e}",
                    method.name
                ))
            })?;
            match stmts.into_iter().next() {
                Some(rustpython_parser::ast::Stmt::FunctionDef(func_node)) => {
                    restored_method_bodies.push((method.name.clone(), func_node.body));
                }
                _ => {
                    return Err(InterpreterError::Security(format!(
                        "imported method '{}' source is not a function definition",
                        method.name
                    )));
                }
            }
        }
    }

    // Restore variables. `SerializedState` uses `BTreeMap` for deterministic
    // wire format; `InterpreterState` internally uses `HashMap` for O(1)
    // variable lookup.
    state.variables = serialized.variables.into_iter().collect();
    state.classes = serialized.classes.into_iter().collect();

    // Publish freshly-parsed function and method bodies into the cache.
    state.function_bodies.clear();
    for (name, body) in restored_bodies {
        state.function_bodies.insert(name, Arc::new(body));
    }
    for (name, body) in restored_method_bodies {
        state.function_bodies.insert(name, Arc::new(body));
    }

    // Re-parse every restored lambda's source and repopulate
    // lambda_bodies. The variable holding a lambda still references
    // it by lambda_id, so the body must land at the SAME key the
    // LambdaDef carries. Walk variables after they're written to
    // state (so we see the typed Value::Lambda variants directly).
    state.lambda_bodies.clear();
    let lambda_imports: Vec<(String, String)> = state
        .variables
        .values()
        .filter_map(|v| match v {
            Value::Lambda(def) => Some((def.lambda_id.clone(), def.source.clone())),
            _ => None,
        })
        .collect();
    for (lambda_id, source) in lambda_imports {
        let parsed = crate::parser::parse(&source).map_err(|e| {
            InterpreterError::Runtime(format!("failed to re-parse lambda '{lambda_id}': {e}"))
        })?;
        // Lambdas are expressions; the wrapped parse produces a
        // single Stmt::Expr holding the lambda. Unwrap to get the
        // ExprLambda's body Expr.
        let Some(rustpython_parser::ast::Stmt::Expr(expr_stmt)) = parsed.into_iter().next() else {
            return Err(InterpreterError::Security(format!(
                "imported lambda '{lambda_id}' source is not an expression statement"
            )));
        };
        let rustpython_parser::ast::Expr::Lambda(lambda_expr) = *expr_stmt.value else {
            return Err(InterpreterError::Security(format!(
                "imported lambda '{lambda_id}' source does not parse as a lambda"
            )));
        };
        state.lambda_bodies.insert(lambda_id, Arc::new(*lambda_expr.body));
    }

    // Recompute memory tracking from imported state.
    state.memory_used_bytes = 0;
    for value in state.variables.values() {
        state.memory_used_bytes =
            state.memory_used_bytes.saturating_add(estimate_value_size(value));
    }
    state.check_memory()?;

    Ok(())
}
