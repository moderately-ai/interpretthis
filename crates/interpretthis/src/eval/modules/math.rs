// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `math` module: real-valued functions and constants.

use std::f64::consts;

use crate::{
    error::{EvalError, EvalResult},
    eval::modules::{arg_f64, need_arg, overflow_error, type_error, value_error},
    value::Value,
};

/// `math` module-level constants.
pub fn constant(name: &str) -> Option<Value> {
    let value = match name {
        "pi" => consts::PI,
        "e" => consts::E,
        "tau" => consts::TAU,
        "inf" => f64::INFINITY,
        "nan" => f64::NAN,
        _ => return None,
    };
    Some(Value::Float(value))
}

/// Whether `math` provides a function named `name`.
pub fn has_function(name: &str) -> bool {
    matches!(
        name,
        "sqrt"
            | "floor"
            | "ceil"
            | "trunc"
            | "fabs"
            | "exp"
            | "log"
            | "log2"
            | "log10"
            | "pow"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "atan2"
            | "hypot"
            | "factorial"
            | "gcd"
            | "isqrt"
            | "radians"
            | "degrees"
            | "isnan"
            | "isinf"
            | "isfinite"
            | "copysign"
            | "fmod"
    )
}

/// Invoke a `math` function.
pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        "sqrt" => {
            let x = arg_f64(func, args, 0)?;
            if x < 0.0 {
                return Err(value_error("math domain error"));
            }
            Ok(Value::Float(x.sqrt()))
        }
        "floor" => Ok(Value::Int(float_to_int(arg_f64(func, args, 0)?.floor())?)),
        "ceil" => Ok(Value::Int(float_to_int(arg_f64(func, args, 0)?.ceil())?)),
        "trunc" => Ok(Value::Int(float_to_int(arg_f64(func, args, 0)?.trunc())?)),
        "fabs" => Ok(Value::Float(arg_f64(func, args, 0)?.abs())),
        "exp" => Ok(Value::Float(arg_f64(func, args, 0)?.exp())),
        "log" => {
            let x = arg_f64(func, args, 0)?;
            if x <= 0.0 {
                return Err(value_error("math domain error"));
            }
            // Two-arg form is log base `b`.
            if args.len() >= 2 {
                Ok(Value::Float(x.log(arg_f64(func, args, 1)?)))
            } else {
                Ok(Value::Float(x.ln()))
            }
        }
        "log2" => Ok(Value::Float(domain_pos(func, args)?.log2())),
        "log10" => Ok(Value::Float(domain_pos(func, args)?.log10())),
        "pow" => Ok(Value::Float(arg_f64(func, args, 0)?.powf(arg_f64(func, args, 1)?))),
        "sin" => Ok(Value::Float(arg_f64(func, args, 0)?.sin())),
        "cos" => Ok(Value::Float(arg_f64(func, args, 0)?.cos())),
        "tan" => Ok(Value::Float(arg_f64(func, args, 0)?.tan())),
        "asin" => Ok(Value::Float(arg_f64(func, args, 0)?.asin())),
        "acos" => Ok(Value::Float(arg_f64(func, args, 0)?.acos())),
        "atan" => Ok(Value::Float(arg_f64(func, args, 0)?.atan())),
        "atan2" => Ok(Value::Float(arg_f64(func, args, 0)?.atan2(arg_f64(func, args, 1)?))),
        "hypot" => {
            // n-dimensional Euclidean norm: sqrt(sum of squares). Two args use
            // the numerically stable `f64::hypot`; other arities sum directly.
            match args.len() {
                2 => Ok(Value::Float(arg_f64(func, args, 0)?.hypot(arg_f64(func, args, 1)?))),
                _ => {
                    let mut sum = 0.0f64;
                    for i in 0..args.len() {
                        let x = arg_f64(func, args, i)?;
                        sum += x * x;
                    }
                    Ok(Value::Float(sum.sqrt()))
                }
            }
        }
        "radians" => Ok(Value::Float(arg_f64(func, args, 0)?.to_radians())),
        "degrees" => Ok(Value::Float(arg_f64(func, args, 0)?.to_degrees())),
        "copysign" => Ok(Value::Float(arg_f64(func, args, 0)?.copysign(arg_f64(func, args, 1)?))),
        "fmod" => {
            let divisor = arg_f64(func, args, 1)?;
            if divisor == 0.0 {
                return Err(value_error("math domain error"));
            }
            Ok(Value::Float(arg_f64(func, args, 0)? % divisor))
        }
        "isnan" => Ok(Value::Bool(arg_f64(func, args, 0)?.is_nan())),
        "isinf" => Ok(Value::Bool(arg_f64(func, args, 0)?.is_infinite())),
        "isfinite" => Ok(Value::Bool(arg_f64(func, args, 0)?.is_finite())),
        "factorial" => factorial(need_arg(func, args, 0)?),
        "gcd" => {
            let a = arg_i64(func, args, 0)?;
            let b = arg_i64(func, args, 1)?;
            Ok(Value::Int(gcd(a.unsigned_abs(), b.unsigned_abs())))
        }
        "isqrt" => {
            let arg = need_arg(func, args, 0)?;
            let Some(n) = crate::value::value_as_bigint(arg) else {
                return Err(type_error(format!(
                    "'{}' object cannot be interpreted as an integer",
                    arg.type_name()
                )));
            };
            if n.sign() == num_bigint::Sign::Minus {
                return Err(value_error("isqrt() argument must be nonnegative"));
            }
            Ok(crate::value::int_from_bigint(num_integer::Roots::sqrt(&n)))
        }
        _ => Err(crate::error::InterpreterError::AttributeError(format!(
            "module 'math' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// Read a positive argument for the logarithm family, enforcing the domain.
fn domain_pos(func: &str, args: &[Value]) -> Result<f64, EvalError> {
    let x = arg_f64(func, args, 0)?;
    if x <= 0.0 {
        return Err(value_error("math domain error"));
    }
    Ok(x)
}

fn arg_i64(func: &str, args: &[Value], index: usize) -> Result<i64, EvalError> {
    let _ = func;
    let _ = index;
    match need_arg(func, args, index)? {
        Value::Int(i) => Ok(*i),
        Value::Bool(b) => Ok(i64::from(*b)),
        // CPython 3.12 says `'<type>' object cannot be interpreted as
        // an integer` — same wording for every callsite that expects
        // an integral argument (isqrt, gcd, factorial(float)).
        other => Err(type_error(format!(
            "'{}' object cannot be interpreted as an integer",
            other.type_name()
        ))),
    }
}

/// Convert a `math.floor`/`ceil`/`trunc` result to `i64` (Python returns int).
#[expect(
    clippy::cast_possible_truncation,
    reason = "floor/ceil/trunc already produced an integral f64; out-of-i64-range \
              values saturate, matching the lossy boundary of a fixed-width int"
)]
fn float_to_int(f: f64) -> Result<i64, EvalError> {
    // CPython: infinity → OverflowError, NaN → ValueError. Two
    // different exception types because the failure modes are
    // semantically distinct (overflow vs invalid value).
    if f.is_infinite() {
        return Err(overflow_error("cannot convert float infinity to integer"));
    }
    if f.is_nan() {
        return Err(value_error("cannot convert float NaN to integer"));
    }
    Ok(f as i64)
}

fn factorial(arg: &Value) -> EvalResult {
    let n = match arg {
        Value::Int(i) => *i,
        Value::Bool(b) => i64::from(*b),
        // CPython 3.12: `factorial(2.5)` raises TypeError, not
        // ValueError. The float is wrong-type, not out-of-range.
        Value::Float(_) => {
            return Err(type_error("'float' object cannot be interpreted as an integer"));
        }
        other => {
            return Err(type_error(format!(
                "'{}' object cannot be interpreted as an integer",
                other.type_name()
            )));
        }
    };
    if n < 0 {
        return Err(value_error("factorial() not defined for negative values"));
    }
    let mut result: i64 = 1;
    for k in 2..=n {
        result =
            result.checked_mul(k).ok_or_else(|| value_error("factorial() result overflows"))?;
    }
    Ok(Value::Int(result))
}

const fn gcd(mut a: u64, mut b: u64) -> i64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    // gcd of two i64 magnitudes fits in i64 (≤ the larger magnitude).
    #[expect(
        clippy::cast_possible_wrap,
        reason = "result is the gcd of two i64 magnitudes, always ≤ i64::MAX"
    )]
    let result = a as i64;
    result
}

/// `math` module registration.
pub struct MathModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for MathModule {
    fn name(&self) -> &'static str {
        "math"
    }
    fn constant(&self, name: &str) -> Option<Value> {
        constant(name)
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
