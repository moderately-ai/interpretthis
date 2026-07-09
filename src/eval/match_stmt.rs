// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Structural pattern matching (`match` / `case`, PEP 634).
//!
//! Supported patterns: literal/value (`case 1`, `case CONST`), singleton
//! (`case None`/`True`/`False`), capture and wildcard (`case x`, `case _`),
//! as-bindings (`case [1, 2] as pair`), or-patterns (`case 1 | 2`), sequence
//! patterns with an optional star (`case [a, *rest]`), mapping patterns
//! (`case {"k": v, **rest}`), and class patterns (`case Point(x, y)` /
//! `case Point(x=1, y=2)`). Builtin class patterns follow PEP 634's
//! special-cased single-positional shape (`case int(x):` captures the
//! whole value if it's an int); user-class patterns walk the registered
//! `__match_args__` for positional sub-patterns and resolve keyword
//! sub-patterns by attribute name.
//!
//! Captured names are collected during matching and bound only once a case
//! matches in full, then the guard (if any) is evaluated with those bindings
//! visible; a failing guard moves to the next case with the bindings left in
//! place, mirroring CPython.

use std::{future::Future, pin::Pin};

use indexmap::IndexMap;
use rustpython_parser::ast;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{eval_body, eval_expr, functions::resolve_proxy, literals::value_to_key},
    state::InterpreterState,
    tools::Tools,
    value::{Value, ValueKey, shared_list},
};

/// Evaluate a `match` statement, running the first case whose pattern matches
/// the subject and whose guard holds.
pub async fn eval_match(
    state: &mut InterpreterState,
    node: &ast::StmtMatch,
    tools: &Tools,
) -> EvalResult {
    let subject = eval_expr(state, &node.subject, tools).await?;
    let subject = resolve_proxy(&subject).await?;

    for case in &node.cases {
        let mut bindings: Vec<(String, Value)> = Vec::new();
        if pattern_matches(state, &case.pattern, &subject, &mut bindings, tools).await? {
            for (name, value) in bindings {
                state.set_variable(&name, value).map_err(EvalError::Interpreter)?;
            }
            if let Some(ref guard) = case.guard {
                let pass = eval_expr(state, guard, tools).await?;
                if !pass.is_truthy() {
                    continue;
                }
            }
            return eval_body(state, &case.body, tools).await;
        }
    }
    Ok(Value::None)
}

/// Test `pattern` against `subject`, accumulating capture bindings. Returns
/// whether the pattern matched.
fn pattern_matches<'a>(
    state: &'a mut InterpreterState,
    pattern: &'a ast::Pattern,
    subject: &'a Value,
    bindings: &'a mut Vec<(String, Value)>,
    tools: &'a Tools,
) -> Pin<Box<dyn Future<Output = Result<bool, EvalError>> + Send + 'a>> {
    Box::pin(async move {
        match pattern {
            ast::Pattern::MatchValue(p) => {
                let v = eval_expr(state, &p.value, tools).await?;
                Ok(crate::eval::operations::values_equal_pub(&v, subject))
            }
            ast::Pattern::MatchSingleton(p) => {
                let v = crate::eval::literals::eval_constant(&p.value);
                Ok(crate::eval::operations::values_equal_pub(&v, subject))
            }
            ast::Pattern::MatchAs(p) => {
                let matched = match &p.pattern {
                    Some(inner) => pattern_matches(state, inner, subject, bindings, tools).await?,
                    // A bare capture (`case x`) or wildcard (`case _`) always
                    // matches; only the capture binds.
                    None => true,
                };
                if matched {
                    if let Some(name) = &p.name {
                        bindings.push((name.as_str().to_string(), subject.clone()));
                    }
                }
                Ok(matched)
            }
            ast::Pattern::MatchOr(p) => {
                for alt in &p.patterns {
                    let mut alt_bindings = Vec::new();
                    if pattern_matches(state, alt, subject, &mut alt_bindings, tools).await? {
                        bindings.extend(alt_bindings);
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            ast::Pattern::MatchSequence(p) => {
                match_sequence(state, &p.patterns, subject, bindings, tools).await
            }
            ast::Pattern::MatchMapping(p) => {
                match_mapping(state, p, subject, bindings, tools).await
            }
            // A star outside a sequence is a syntax error the parser rejects;
            // reaching here would be an interpreter bug, so treat it as no-match.
            ast::Pattern::MatchStar(_) => Ok(false),
            ast::Pattern::MatchClass(p) => match_class(state, p, subject, bindings, tools).await,
        }
    })
}

/// Match a class pattern (`case Point(x, y)` / `case Point(x=1, y=2)`).
///
/// Resolves the class name; isinstance-checks the subject; then walks
/// positional sub-patterns against the class's `__match_args__`
/// (builtins use PEP-634's single-positional shape) and keyword
/// sub-patterns against named attributes. Each sub-pattern is itself
/// matched recursively, so nested class / capture / sequence patterns
/// compose.
async fn match_class(
    state: &mut InterpreterState,
    pattern: &ast::PatternMatchClass,
    subject: &Value,
    bindings: &mut Vec<(String, Value)>,
    tools: &Tools,
) -> Result<bool, EvalError> {
    // Resolve the class expression. The common forms are a bare Name
    // (`Point`) and a module attribute (`collections.Counter`). Anything
    // else is rejected with a clear error.
    let cls_name = match pattern.cls.as_ref() {
        ast::Expr::Name(n) => n.id.as_str().to_string(),
        ast::Expr::Attribute(a) => a.attr.as_str().to_string(),
        _ => {
            return Err(InterpreterError::Runtime(
                "class pattern class must be a bare name or module attribute".into(),
            )
            .into());
        }
    };

    // isinstance check. Builtin types match against the runtime
    // type_name (`int`, `str`, ...); user classes walk the MRO via
    // check_isinstance-equivalent logic inlined here so we don't have
    // to expose it across modules.
    if !subject_is_instance_of(state, subject, &cls_name) {
        return Ok(false);
    }

    // Builtins: single positional pattern binds the whole subject;
    // keyword patterns are not supported (CPython's `int.__match_args__`
    // is empty, so this catches that case explicitly with a clearer
    // error than "no match" would give).
    let builtin_single_positional = matches!(
        cls_name.as_str(),
        "int"
            | "str"
            | "float"
            | "bool"
            | "bytes"
            | "bytearray"
            | "dict"
            | "frozenset"
            | "list"
            | "set"
            | "tuple"
            | "Counter"
    );
    if builtin_single_positional {
        if pattern.patterns.len() > 1 {
            return Err(InterpreterError::TypeError(format!(
                "{cls_name}() accepts 0 positional sub-patterns ({} given)",
                pattern.patterns.len()
            ))
            .into());
        }
        if let Some(first) = pattern.patterns.first() {
            if !pattern_matches(state, first, subject, bindings, tools).await? {
                return Ok(false);
            }
        }
        // Keyword sub-patterns on builtins: bind by attribute name on
        // the subject. Useful for `case datetime.date(year=2026)`.
        for (name, sub) in pattern.kwd_attrs.iter().zip(&pattern.kwd_patterns) {
            let attr_val = lookup_attribute_for_match(state, subject, name.as_str())?;
            if !pattern_matches(state, sub, &attr_val, bindings, tools).await? {
                return Ok(false);
            }
        }
        return Ok(true);
    }

    // User class: positional sub-patterns require __match_args__ on the
    // class. CPython raises TypeError when positional patterns are
    // present but __match_args__ is missing or is not a tuple of strs.
    if !pattern.patterns.is_empty() {
        let match_args = lookup_match_args(state, &cls_name)?;
        if pattern.patterns.len() > match_args.len() {
            return Err(InterpreterError::TypeError(format!(
                "{cls_name}() accepts {} positional sub-patterns ({} given)",
                match_args.len(),
                pattern.patterns.len()
            ))
            .into());
        }
        // Check for duplicate attribute reference between positional
        // and keyword patterns — CPython raises TypeError on conflict.
        for (idx, sub) in pattern.patterns.iter().enumerate() {
            let attr_name = &match_args[idx];
            if pattern.kwd_attrs.iter().any(|k| k.as_str() == attr_name) {
                return Err(InterpreterError::TypeError(format!(
                    "{cls_name}() got multiple sub-patterns for attribute '{attr_name}'"
                ))
                .into());
            }
            let attr_val = lookup_attribute_for_match(state, subject, attr_name)?;
            if !pattern_matches(state, sub, &attr_val, bindings, tools).await? {
                return Ok(false);
            }
        }
    }

    // Keyword sub-patterns: match the subject's attribute by name.
    for (name, sub) in pattern.kwd_attrs.iter().zip(&pattern.kwd_patterns) {
        let attr_val = lookup_attribute_for_match(state, subject, name.as_str())?;
        if !pattern_matches(state, sub, &attr_val, bindings, tools).await? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Sync isinstance check for the class-pattern path. Replicates
/// functions.rs::check_isinstance without crossing modules; the
/// inlined logic is the same — builtin pairs match by type name +
/// bool-is-int + Counter-is-dict, user classes walk the MRO.
fn subject_is_instance_of(state: &InterpreterState, obj: &Value, type_name: &str) -> bool {
    if type_name == "object" {
        return true;
    }
    if let Value::Instance(inst) = obj {
        if inst.class_name == type_name {
            return true;
        }
        if let Some(class) = state.classes.get(&inst.class_name) {
            return class.mro.iter().any(|ancestor| ancestor == type_name);
        }
        return false;
    }
    obj.type_name() == type_name
        || matches!((obj, type_name), (Value::Bool(_), "int") | (Value::Counter(_), "dict"))
}

/// Read `__match_args__` from a registered user class. Expected shape
/// is a tuple of string attribute names. Empty or missing yields an
/// empty Vec (matches CPython's default `object.__match_args__ = ()`).
fn lookup_match_args(
    state: &InterpreterState,
    class_name: &str,
) -> Result<Vec<compact_str::CompactString>, EvalError> {
    let Some(class) = state.classes.get(class_name) else {
        return Ok(Vec::new());
    };
    let Some(value) = class.class_attrs.get("__match_args__") else {
        return Ok(Vec::new());
    };
    let Value::Tuple(items) = value else {
        return Err(InterpreterError::TypeError(format!(
            "{class_name}.__match_args__ must be a tuple (got '{}')",
            value.type_name()
        ))
        .into());
    };
    let mut names = Vec::with_capacity(items.len());
    for item in items {
        let Value::String(s) = item else {
            return Err(InterpreterError::TypeError(format!(
                "{class_name}.__match_args__ entries must be strings (got '{}')",
                item.type_name()
            ))
            .into());
        };
        names.push(s.clone());
    }
    Ok(names)
}

/// Read an attribute off the subject for class-pattern matching. For
/// user-class instances this walks the registry; for builtins we read
/// the conventional public attributes (e.g. `.year` on Date).
fn lookup_attribute_for_match(state: &InterpreterState, subject: &Value, attr: &str) -> EvalResult {
    if let Value::Instance(inst) = subject {
        return crate::eval::classes::instance_attribute(state, inst, attr);
    }
    if let Value::Date(date) = subject {
        return crate::eval::modules::datetime::date_attribute(*date, attr);
    }
    Err(InterpreterError::AttributeError(format!(
        "'{}' object has no attribute '{attr}'",
        subject.type_name()
    ))
    .into())
}

/// Match a sequence pattern (`[a, b]`, `(a, *rest, b)`) against a list or tuple.
/// Strings and bytes are deliberately excluded, matching CPython.
async fn match_sequence(
    state: &mut InterpreterState,
    patterns: &[ast::Pattern],
    subject: &Value,
    bindings: &mut Vec<(String, Value)>,
    tools: &Tools,
) -> Result<bool, EvalError> {
    let items: Vec<Value> = match subject {
        // List is shared via Arc<Mutex<Vec>>; clone the snapshot under
        // the lock so pattern matching sees a stable sequence.
        Value::List(items) => items.lock().clone(),
        Value::Tuple(items) => items.clone(),
        _ => return Ok(false),
    };

    let star_pos = patterns.iter().position(|p| matches!(p, ast::Pattern::MatchStar(_)));

    let Some(star) = star_pos else {
        if items.len() != patterns.len() {
            return Ok(false);
        }
        for (pat, item) in patterns.iter().zip(&items) {
            if !pattern_matches(state, pat, item, bindings, tools).await? {
                return Ok(false);
            }
        }
        return Ok(true);
    };

    let before = &patterns[..star];
    let after = &patterns[star + 1..];
    if items.len() < before.len() + after.len() {
        return Ok(false);
    }
    for (pat, item) in before.iter().zip(&items) {
        if !pattern_matches(state, pat, item, bindings, tools).await? {
            return Ok(false);
        }
    }
    let after_start = items.len() - after.len();
    for (pat, item) in after.iter().zip(&items[after_start..]) {
        if !pattern_matches(state, pat, item, bindings, tools).await? {
            return Ok(false);
        }
    }
    if let ast::Pattern::MatchStar(s) = &patterns[star] {
        if let Some(name) = &s.name {
            let middle = items[before.len()..after_start].to_vec();
            bindings.push((name.as_str().to_string(), Value::List(shared_list(middle))));
        }
    }
    Ok(true)
}

/// Match a mapping pattern (`{"k": v, **rest}`) against a dict. Every listed key
/// must be present and its value must match the sub-pattern; `**rest` binds the
/// unmatched entries.
async fn match_mapping(
    state: &mut InterpreterState,
    pattern: &ast::PatternMatchMapping,
    subject: &Value,
    bindings: &mut Vec<(String, Value)>,
    tools: &Tools,
) -> Result<bool, EvalError> {
    let Value::Dict(map) = subject else {
        return Ok(false);
    };
    let map = map.clone();

    let mut matched_keys: Vec<ValueKey> = Vec::new();
    for (key_expr, sub) in pattern.keys.iter().zip(&pattern.patterns) {
        let key_val = eval_expr(state, key_expr, tools).await?;
        let key = value_to_key(&key_val)?;
        let Some(value) = map.get(&key).cloned() else {
            return Ok(false);
        };
        if !pattern_matches(state, sub, &value, bindings, tools).await? {
            return Ok(false);
        }
        matched_keys.push(key);
    }

    if let Some(rest) = &pattern.rest {
        let remaining: IndexMap<ValueKey, Value> = map
            .iter()
            .filter(|(k, _)| !matched_keys.contains(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        bindings.push((rest.as_str().to_string(), Value::Dict(remaining)));
    }
    Ok(true)
}
