// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `cmath` module — complex-valued math functions.
//!
//! Real arguments are promoted to `complex`; results that are complex stay
//! complex, `phase`/`abs`-style reductions return a float, and `polar`
//! returns a `(modulus, phase)` tuple.

use num_complex::Complex64;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    value::Value,
};

/// `cmath` module-level constants.
pub fn constant(name: &str) -> Option<Value> {
    match name {
        "pi" => Some(Value::Float(std::f64::consts::PI)),
        "e" => Some(Value::Float(std::f64::consts::E)),
        "tau" => Some(Value::Float(std::f64::consts::TAU)),
        "inf" => Some(Value::Float(f64::INFINITY)),
        "nan" => Some(Value::Float(f64::NAN)),
        "infj" => Some(Value::Complex(Box::new(Complex64::new(0.0, f64::INFINITY)))),
        "nanj" => Some(Value::Complex(Box::new(Complex64::new(0.0, f64::NAN)))),
        _ => None,
    }
}

pub fn has_function(name: &str) -> bool {
    matches!(
        name,
        "sqrt"
            | "exp"
            | "log"
            | "log10"
            | "phase"
            | "polar"
            | "rect"
            | "sin"
            | "cos"
            | "tan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "asin"
            | "acos"
            | "atan"
            | "isnan"
            | "isinf"
            | "isfinite"
            | "isclose"
    )
}

/// Coerce an argument (int / float / complex / bool) to a `Complex64`.
fn arg_complex(func: &str, args: &[Value], index: usize) -> Result<Complex64, EvalError> {
    match args.get(index) {
        Some(Value::Complex(c)) => Ok(**c),
        Some(Value::Float(f)) => Ok(Complex64::new(*f, 0.0)),
        Some(Value::Int(i)) => Ok(Complex64::new(*i as f64, 0.0)),
        Some(Value::Bool(b)) => Ok(Complex64::new(f64::from(*b), 0.0)),
        Some(Value::BigInt(b)) => {
            use num_traits::ToPrimitive as _;
            Ok(Complex64::new(b.to_f64().unwrap_or(f64::INFINITY), 0.0))
        }
        _ => {
            Err(InterpreterError::TypeError(format!("cmath.{func}() argument must be a number"))
                .into())
        }
    }
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    let c = |z: Complex64| Ok(Value::Complex(Box::new(z)));
    match func {
        "sqrt" => c(arg_complex(func, args, 0)?.sqrt()),
        "exp" => c(arg_complex(func, args, 0)?.exp()),
        "log" => {
            let z = arg_complex(func, args, 0)?;
            // Optional second argument is the base.
            match args.get(1) {
                None => c(z.ln()),
                Some(_) => {
                    let base = arg_complex(func, args, 1)?;
                    c(z.ln() / base.ln())
                }
            }
        }
        "log10" => c(arg_complex(func, args, 0)?.log10()),
        "sin" => c(arg_complex(func, args, 0)?.sin()),
        "cos" => c(arg_complex(func, args, 0)?.cos()),
        "tan" => c(arg_complex(func, args, 0)?.tan()),
        "sinh" => c(arg_complex(func, args, 0)?.sinh()),
        "cosh" => c(arg_complex(func, args, 0)?.cosh()),
        "tanh" => c(arg_complex(func, args, 0)?.tanh()),
        "asin" => c(arg_complex(func, args, 0)?.asin()),
        "acos" => c(arg_complex(func, args, 0)?.acos()),
        "atan" => c(arg_complex(func, args, 0)?.atan()),
        // `phase(z)` is the argument angle (a float); `polar` returns
        // `(modulus, phase)`; `rect(r, phi)` reconstructs the complex.
        "phase" => Ok(Value::Float(arg_complex(func, args, 0)?.arg())),
        "polar" => {
            let (r, theta) = arg_complex(func, args, 0)?.to_polar();
            Ok(Value::Tuple(vec![Value::Float(r), Value::Float(theta)]))
        }
        "rect" => {
            let r = as_f64(func, args, 0)?;
            let phi = as_f64(func, args, 1)?;
            c(Complex64::from_polar(r, phi))
        }
        "isnan" => {
            let z = arg_complex(func, args, 0)?;
            Ok(Value::Bool(z.re.is_nan() || z.im.is_nan()))
        }
        "isinf" => {
            let z = arg_complex(func, args, 0)?;
            Ok(Value::Bool(z.re.is_infinite() || z.im.is_infinite()))
        }
        "isfinite" => {
            let z = arg_complex(func, args, 0)?;
            Ok(Value::Bool(z.re.is_finite() && z.im.is_finite()))
        }
        "isclose" => {
            let a = arg_complex(func, args, 0)?;
            let b = arg_complex(func, args, 1)?;
            // Default rel_tol=1e-09, abs_tol=0.0 (CPython).
            let diff = (a - b).norm();
            let tol = (1e-9 * a.norm().max(b.norm())).max(0.0);
            Ok(Value::Bool(diff <= tol))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'cmath' has no attribute '{func}'"
        ))
        .into()),
    }
}

fn as_f64(func: &str, args: &[Value], index: usize) -> Result<f64, EvalError> {
    match args.get(index) {
        Some(Value::Float(f)) => Ok(*f),
        Some(Value::Int(i)) => Ok(*i as f64),
        Some(Value::Bool(b)) => Ok(f64::from(*b)),
        _ => Err(InterpreterError::TypeError(format!(
            "cmath.{func}() argument must be a real number"
        ))
        .into()),
    }
}

pub struct CmathModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for CmathModule {
    fn name(&self) -> &'static str {
        "cmath"
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
