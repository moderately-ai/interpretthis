// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `statistics` module for numeric sequences.
//!
//! CPython's `statistics` uses exact `Fraction` arithmetic and returns an `int`
//! when the result of an all-integer input is integral (e.g. `mean([1, 2, 3])`
//! is `2`, not `2.0`), and a `float` otherwise. The functions here reproduce
//! that result typing: integral results from all-integer data come back as
//! `Int`, everything else as `Float`. `median` of an odd-length sequence returns
//! the middle element unchanged (preserving its type); `stdev`/`pstdev` are
//! always floats (a square root).

#![expect(
    clippy::cast_precision_loss,
    reason = "statistics divides by and weights element counts as f64; the count \
              would have to exceed 2^52 elements to lose precision, which the \
              operation/memory limits make impossible"
)]

use std::cmp::Ordering;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{control_flow::iterate_value, modules::need_arg},
    value::Value,
};

/// Whether `statistics` provides a function named `name`.
pub fn has_function(name: &str) -> bool {
    matches!(name, "mean" | "median" | "stdev" | "variance" | "pstdev" | "pvariance" | "mode")
}

/// Invoke a `statistics` function.
pub fn call(func: &str, args: &[Value]) -> EvalResult {
    let data = iterate_value(need_arg(func, args, 0)?)?;
    match func {
        "mean" => {
            let nums = numbers(func, &data, 1)?;
            Ok(coerce(mean(&nums), all_integer(&data)))
        }
        "median" => median(&data),
        // variance / stdev require >= 2 data points; CPython's
        // message is "requires at least two data points" with the
        // OUTER function name, not the inner variance() helper.
        "variance" => {
            let nums = numbers("variance", &data, 2)?;
            Ok(coerce(variance(&nums, true), all_integer(&data)))
        }
        "stdev" => Ok(Value::Float(variance(&numbers("stdev", &data, 2)?, true).sqrt())),
        // pvariance / pstdev only need >= 1 (population variance is
        // defined for a single point, just zero).
        "pvariance" => {
            let nums = numbers("pvariance", &data, 1)?;
            Ok(coerce(variance(&nums, false), all_integer(&data)))
        }
        "pstdev" => Ok(Value::Float(variance(&numbers("pstdev", &data, 1)?, false).sqrt())),
        "mode" => mode(&data),
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'statistics' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// Coerce a numeric result to CPython's return type: an integral result from
/// all-integer input becomes an `Int`; otherwise a `Float`.
fn coerce(value: f64, all_integer_input: bool) -> Value {
    if all_integer_input && value.is_finite() && value.fract() == 0.0 {
        if let Some(as_int) = exact_i64(value) {
            return Value::Int(as_int);
        }
    }
    Value::Float(value)
}

/// `value` as an `i64` iff the conversion round-trips exactly.
#[expect(
    clippy::cast_possible_truncation,
    clippy::float_cmp,
    reason = "exact round-trip guard: returns Some only when `i as f64 == value`, where \
              exact equality is the intended check and the truncating cast is therefore exact"
)]
fn exact_i64(value: f64) -> Option<i64> {
    let as_int = value as i64;
    (as_int as f64 == value).then_some(as_int)
}

/// Whether every element is an `int` (or `bool`, an int subclass) — the
/// condition under which CPython's stats return an `int`.
fn all_integer(data: &[Value]) -> bool {
    !data.is_empty() && data.iter().all(|v| matches!(v, Value::Int(_) | Value::Bool(_)))
}

/// Coerce a data sequence to `f64`s, validating `min_required` first
/// (CPython errors at the data-points threshold before attempting any
/// type conversion). Phrases the message as "one" or "two" data
/// points to match CPython's wording exactly.
fn numbers(func: &str, data: &[Value], min_required: usize) -> Result<Vec<f64>, EvalError> {
    if data.len() < min_required {
        let qualifier = if min_required <= 1 { "one data point" } else { "two data points" };
        return Err(crate::eval::modules::statistics_error(format!(
            "{func} requires at least {qualifier}"
        )));
    }
    data.iter()
        .map(|v| {
            v.as_float().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError("can't convert value to float".into()))
            })
        })
        .collect()
}

fn mean(data: &[f64]) -> f64 {
    let sum: f64 = data.iter().sum();
    sum / data.len() as f64
}

fn median(data: &[Value]) -> EvalResult {
    if data.is_empty() {
        return Err(crate::eval::modules::statistics_error("no median for empty data"));
    }
    // Order the original values numerically so an odd-length median can return
    // the middle element unchanged (preserving int vs float), as CPython does.
    let mut ordered: Vec<&Value> = data.iter().collect();
    ordered.sort_by(|a, b| {
        let av = a.as_float().unwrap_or(f64::NAN);
        let bv = b.as_float().unwrap_or(f64::NAN);
        av.partial_cmp(&bv).unwrap_or(Ordering::Equal)
    });
    let n = ordered.len();
    let mid = n / 2;
    if n % 2 == 1 {
        Ok(ordered[mid].clone())
    } else {
        // Even length always averages the two central values into a float.
        let lo = ordered[mid - 1].as_float().unwrap_or(f64::NAN);
        let hi = ordered[mid].as_float().unwrap_or(f64::NAN);
        Ok(Value::Float(f64::midpoint(lo, hi)))
    }
}

/// Sample (n-1 denominator) or population (n denominator) variance.
/// Threshold validation is the caller's job — variance assumes the
/// data already passed the right `numbers(..., min_required)` check.
fn variance(data: &[f64], sample: bool) -> f64 {
    let n = data.len();
    let m = mean(data);
    let ss: f64 = data.iter().map(|x| (x - m).powi(2)).sum();
    let denom = if sample { (n - 1) as f64 } else { n as f64 };
    ss / denom
}

fn mode(data: &[Value]) -> EvalResult {
    if data.is_empty() {
        return Err(crate::eval::modules::statistics_error("no mode for empty data"));
    }
    // The first value to reach the highest count wins (CPython 3.8+ returns the
    // first mode encountered). `mode` preserves the element's type. Counts are
    // keyed on the value's repr, unambiguous for the hashable scalars used here.
    let mut counts: indexmap::IndexMap<String, (usize, Value)> = indexmap::IndexMap::new();
    for value in data {
        let entry = counts.entry(value.repr()).or_insert((0, value.clone()));
        entry.0 += 1;
    }
    // CPython 3.8+ returns the *first* value reaching the maximum count. `counts`
    // is insertion-ordered (first appearance), so `max_by_key` would pick the
    // last tie — find the first entry at the max instead.
    let max_count = counts.values().map(|(count, _)| *count).max().unwrap_or(0);
    let best = counts
        .values()
        .find(|(count, _)| *count == max_count)
        .map_or(Value::None, |(_, value)| value.clone());
    Ok(best)
}

/// `statistics` module registration.
pub struct StatisticsModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for StatisticsModule {
    fn name(&self) -> &'static str {
        "statistics"
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
