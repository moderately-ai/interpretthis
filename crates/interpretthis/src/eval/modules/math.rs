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
        // Return the exact integer, promoting past i64 to BigInt (CPython
        // `math.floor(1e30)`); inf -> OverflowError, nan -> ValueError.
        "floor" => crate::eval::functions::float_to_int_exact(arg_f64(func, args, 0)?.floor()),
        "ceil" => crate::eval::functions::float_to_int_exact(arg_f64(func, args, 0)?.ceil()),
        "trunc" => crate::eval::functions::float_to_int_exact(arg_f64(func, args, 0)?.trunc()),
        "fabs" => Ok(Value::Float(arg_f64(func, args, 0)?.abs())),
        "exp" => {
            let x = arg_f64(func, args, 0)?;
            let r = x.exp();
            // A finite argument overflowing to infinity is a range error.
            if r.is_infinite() && x.is_finite() {
                return Err(overflow_error("math range error"));
            }
            Ok(Value::Float(r))
        }
        "log" => {
            let x = arg_f64(func, args, 0)?;
            if x <= 0.0 {
                return Err(value_error("math domain error"));
            }
            // Two-arg form is log base `b`: the base must be positive and not 1.
            if args.len() >= 2 {
                let base = arg_f64(func, args, 1)?;
                if base <= 0.0 {
                    return Err(value_error("math domain error"));
                }
                if base == 1.0 {
                    return Err(EvalError::Exception(crate::value::ExceptionValue::new(
                        "ZeroDivisionError",
                        "float division by zero",
                    )));
                }
                Ok(Value::Float(x.log(base)))
            } else {
                Ok(Value::Float(x.ln()))
            }
        }
        "log2" => Ok(Value::Float(domain_pos(func, args)?.log2())),
        "log10" => Ok(Value::Float(domain_pos(func, args)?.log10())),
        "pow" => {
            let x = arg_f64(func, args, 0)?;
            let y = arg_f64(func, args, 1)?;
            // Zero to a negative power is undefined (checked before computing,
            // where it would otherwise become +inf).
            if x == 0.0 && y < 0.0 {
                return Err(value_error("math domain error"));
            }
            let r = x.powf(y);
            // A NaN from finite operands is an undefined/complex result (e.g. a
            // negative base to a fractional exponent) -> ValueError.
            if r.is_nan() && x.is_finite() && y.is_finite() {
                return Err(value_error("math domain error"));
            }
            // Finite operands overflowing to infinity is a range error.
            if r.is_infinite() && x.is_finite() && y.is_finite() {
                return Err(overflow_error("math range error"));
            }
            Ok(Value::Float(r))
        }
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
            // Variadic and arbitrary-precision: gcd() == 0, and gcd folds
            // pairwise over every argument (num_integer::Integer::gcd yields the
            // non-negative gcd, so signs and one-arg abs fall out for free).
            let mut acc = num_bigint::BigInt::from(0);
            for arg in args {
                let Some(n) = crate::value::value_as_bigint(arg) else {
                    return Err(type_error(format!(
                        "'{}' object cannot be interpreted as an integer",
                        arg.type_name()
                    )));
                };
                acc = num_integer::Integer::gcd(&acc, &n);
            }
            Ok(crate::value::int_from_bigint(acc))
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
