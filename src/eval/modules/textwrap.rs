// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `textwrap` module.
//!
//! Supports `dedent`, `indent`, `wrap`, `fill`, and `shorten` — the
//! cases that appear in LLM-extraction-script reshaping of multi-line
//! strings.

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::modules::arg_str,
    value::{Value, shared_list},
};

pub fn has_function(name: &str) -> bool {
    matches!(name, "dedent" | "indent" | "wrap" | "fill" | "shorten")
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        "dedent" => {
            let s = arg_str(func, args, 0)?;
            Ok(Value::String(dedent(s).into()))
        }
        "indent" => {
            let s = arg_str(func, args, 0)?;
            let prefix = arg_str(func, args, 1)?;
            // CPython's textwrap.indent prefixes every non-empty line.
            // No predicate function support (would require call-back
            // into the evaluator which the module-call shim doesn't
            // thread through).
            let result = s
                .lines()
                .map(
                    |line| {
                        if line.is_empty() { line.to_string() } else { format!("{prefix}{line}") }
                    },
                )
                .collect::<Vec<_>>()
                .join("\n");
            // Preserve trailing newline if present in input.
            let mut out = result;
            if s.ends_with('\n') && !out.ends_with('\n') {
                out.push('\n');
            }
            Ok(Value::String(out.into()))
        }
        "fill" => {
            let s = arg_str(func, args, 0)?;
            let width = opt_width(args, 1).unwrap_or(70);
            Ok(Value::String(fill(s, width).into()))
        }
        "wrap" => {
            let s = arg_str(func, args, 0)?;
            let width = opt_width(args, 1).unwrap_or(70);
            let lines = wrap(s, width).into_iter().map(|w| Value::String(w.into())).collect();
            Ok(Value::List(shared_list(lines)))
        }
        "shorten" => {
            let s = arg_str(func, args, 0)?;
            let width = opt_width(args, 1).ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "shorten() requires a width argument".into(),
                ))
            })?;
            let placeholder = args
                .get(2)
                .and_then(|v| match v {
                    Value::String(s) => Some(s.to_string()),
                    _ => None,
                })
                .unwrap_or_else(|| " [...]".to_string());
            Ok(Value::String(shorten(s, width, &placeholder).into()))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'textwrap' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// `textwrap.dedent`: strip the common leading whitespace from each
/// non-empty line.
fn dedent(s: &str) -> String {
    let non_empty: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    if non_empty.is_empty() {
        return s.to_string();
    }
    let common = non_empty
        .iter()
        .map(|line| line.chars().take_while(|c| c.is_whitespace() && *c != '\n').count())
        .min()
        .unwrap_or(0);
    let mut out = String::new();
    for (i, line) in s.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if line.trim().is_empty() {
            out.push_str(line);
        } else {
            // Skip the common-leading-whitespace prefix.
            let mut chars = line.chars();
            for _ in 0..common {
                chars.next();
            }
            out.push_str(chars.as_str());
        }
    }
    if s.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// `textwrap.wrap`: split `text` into a list of lines, each ≤ `width`
/// characters, breaking at whitespace boundaries. Simplified vs
/// CPython's full algorithm — adequate for typical wrapping.
fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            if word.len() > width {
                // A single word longer than the width gets its own
                // line (CPython does not break_long_words by default
                // beyond this).
                lines.push(word.to_string());
            } else {
                current.push_str(word);
            }
        } else if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current.clone());
            current.clear();
            if word.len() > width {
                lines.push(word.to_string());
            } else {
                current.push_str(word);
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn fill(text: &str, width: usize) -> String {
    wrap(text, width).join("\n")
}

fn shorten(text: &str, width: usize, placeholder: &str) -> String {
    // CPython's shorten collapses whitespace then truncates.
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() <= width {
        return collapsed;
    }
    if placeholder.len() >= width {
        return placeholder.to_string();
    }
    let available = width.saturating_sub(placeholder.len());
    // Trim back to word boundary.
    let cap = available.min(collapsed.len());
    let truncated = collapsed[..cap]
        .rfind(' ')
        .map_or_else(|| collapsed[..cap].to_string(), |idx| collapsed[..idx].to_string());
    format!("{truncated}{placeholder}")
}

fn opt_width(args: &[Value], index: usize) -> Option<usize> {
    match args.get(index)? {
        Value::Int(n) => usize::try_from(*n).ok(),
        Value::Bool(b) => Some(usize::from(*b)),
        _ => None,
    }
}

/// `textwrap` module registration.
pub struct TextwrapModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for TextwrapModule {
    fn name(&self) -> &'static str {
        "textwrap"
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
