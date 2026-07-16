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
            | "nextafter"
            | "ulp"
            | "fmod"
            | "comb"
            | "perm"
            | "prod"
            | "fsum"
            | "dist"
            | "lcm"
            | "isclose"
            | "expm1"
            | "log1p"
            | "remainder"
            | "ldexp"
            | "frexp"
            | "modf"
            | "gamma"
            | "lgamma"
            | "erf"
            | "erfc"
            | "sinh"
            | "cosh"
            | "tanh"
    )
}

/// Invoke a `math` function.
pub fn call(func: &str, args: &[Value], kwargs: &indexmap::IndexMap<String, Value>) -> EvalResult {
    match func {
        // IEEE 754 remainder: x - round-half-even(x / y) * y.
        "remainder" => {
            let (x, y) = (arg_f64(func, args, 0)?, arg_f64(func, args, 1)?);
            Ok(Value::Float(x - (x / y).round_ties_even() * y))
        }
        "ldexp" => {
            let m = arg_f64(func, args, 0)?;
            let e = arg_f64(func, args, 1)?.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
            Ok(Value::Float(m * 2f64.powi(e)))
        }
        // frexp(x) = (m, e) with x == m * 2**e and 0.5 <= |m| < 1 (or (0.0, 0)).
        "frexp" => {
            let x = arg_f64(func, args, 0)?;
            let (m, e) = if x == 0.0 || !x.is_finite() {
                (x, 0)
            } else {
                let mut e = x.abs().log2().floor() as i32 + 1;
                let mut m = x / 2f64.powi(e);
                while m.abs() >= 1.0 {
                    m /= 2.0;
                    e += 1;
                }
                while m.abs() < 0.5 {
                    m *= 2.0;
                    e -= 1;
                }
                (m, e)
            };
            Ok(Value::Tuple(vec![Value::Float(m), Value::Int(i64::from(e))]))
        }
        // modf(x) = (fractional, integral) parts, both floats, sign of x.
        "modf" => {
            let x = arg_f64(func, args, 0)?;
            let int_part = x.trunc();
            Ok(Value::Tuple(vec![Value::Float(x - int_part), Value::Float(int_part)]))
        }
        "gamma" => Ok(Value::Float(gamma(arg_f64(func, args, 0)?))),
        "lgamma" => Ok(Value::Float(gamma(arg_f64(func, args, 0)?).abs().ln())),
        "erf" => Ok(Value::Float(erf(arg_f64(func, args, 0)?))),
        "erfc" => Ok(Value::Float(erfc(arg_f64(func, args, 0)?))),
        "sinh" => Ok(Value::Float(arg_f64(func, args, 0)?.sinh())),
        "cosh" => Ok(Value::Float(arg_f64(func, args, 0)?.cosh())),
        "tanh" => Ok(Value::Float(arg_f64(func, args, 0)?.tanh())),
        "comb" => math_comb(args),
        "perm" => math_perm(args),
        "prod" => math_prod(args, kwargs),
        "fsum" => math_fsum(args),
        "dist" => math_dist(args),
        "lcm" => math_lcm(args),
        "isclose" => math_isclose(func, args, kwargs),
        "expm1" => Ok(Value::Float(arg_f64(func, args, 0)?.exp_m1())),
        "log1p" => Ok(Value::Float(arg_f64(func, args, 0)?.ln_1p())),
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
        // asin/acos are defined only on [-1, 1]; outside it CPython raises
        // (Rust's f64 would silently return NaN).
        "asin" => {
            let x = arg_f64(func, args, 0)?;
            if !(-1.0..=1.0).contains(&x) {
                return Err(value_error("math domain error"));
            }
            Ok(Value::Float(x.asin()))
        }
        "acos" => {
            let x = arg_f64(func, args, 0)?;
            if !(-1.0..=1.0).contains(&x) {
                return Err(value_error("math domain error"));
            }
            Ok(Value::Float(x.acos()))
        }
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
        // `nextafter(x, y)` — the next representable double after x toward y.
        "nextafter" => {
            Ok(Value::Float(next_after(arg_f64(func, args, 0)?, arg_f64(func, args, 1)?)))
        }
        // `ulp(x)` — the value of the least significant bit of x.
        "ulp" => {
            let x = arg_f64(func, args, 0)?.abs();
            let out = if x.is_nan() || x.is_infinite() {
                x
            } else if x == 0.0 {
                f64::from_bits(1)
            } else {
                next_after(x, f64::INFINITY) - x
            };
            Ok(Value::Float(out))
        }
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

/// A non-negative integer argument (for `comb`/`perm`), promoting past i64.
fn nonneg_int(func: &str, args: &[Value], index: usize) -> Result<num_bigint::BigInt, EvalError> {
    let arg = need_arg(func, args, index)?;
    let n = crate::value::value_as_bigint(arg).ok_or_else(|| {
        type_error(format!("'{}' object cannot be interpreted as an integer", arg.type_name()))
    })?;
    if n.sign() == num_bigint::Sign::Minus {
        return Err(value_error(format!("{func}() argument must be a non-negative integer")));
    }
    Ok(n)
}

/// `math.comb(n, k)` — binomial coefficient via the exact multiplicative
/// formula (each partial result stays integral, so integer division is exact).
fn math_comb(args: &[Value]) -> EvalResult {
    use num_bigint::BigInt;
    let n = nonneg_int("comb", args, 0)?;
    let mut k = nonneg_int("comb", args, 1)?;
    if k > n {
        return Ok(Value::Int(0));
    }
    // Use the smaller of k and n-k for fewer iterations (symmetry).
    let nk = &n - &k;
    if nk < k {
        k = nk;
    }
    let mut result = BigInt::from(1);
    let mut i = BigInt::from(0);
    while i < k {
        result = result * (&n - &k + &i + 1u32) / (&i + 1u32);
        i += 1u32;
    }
    Ok(crate::value::int_from_bigint(result))
}

/// `math.perm(n, k=None)` — falling factorial `n! / (n-k)!`; `k=None` gives `n!`.
fn math_perm(args: &[Value]) -> EvalResult {
    use num_bigint::BigInt;
    let n = nonneg_int("perm", args, 0)?;
    let k = match args.get(1) {
        None | Some(Value::None) => n.clone(),
        Some(_) => nonneg_int("perm", args, 1)?,
    };
    if k > n {
        return Ok(Value::Int(0));
    }
    let mut result = BigInt::from(1);
    let mut i = BigInt::from(0);
    while i < k {
        result *= &n - &i;
        i += 1u32;
    }
    Ok(crate::value::int_from_bigint(result))
}

/// `math.prod(iterable, *, start=1)` — product of the elements, routed through
/// the arithmetic kernel so int/float/BigInt promotion matches `*`.
fn math_prod(args: &[Value], kwargs: &indexmap::IndexMap<String, Value>) -> EvalResult {
    let items = crate::eval::control_flow::iterate_value(need_arg("prod", args, 0)?)?;
    let mut acc = kwargs.get("start").cloned().unwrap_or(Value::Int(1));
    for item in items {
        acc = crate::eval::operations::apply_binop_builtin(crate::types::BinOp::Mul, &acc, &item)?;
    }
    Ok(acc)
}

/// `math.fsum(iterable)` — exact floating-point sum via Shewchuk's algorithm,
/// tracking non-overlapping partial sums so the result is correctly rounded
/// (a naive left-fold of `[0.1] * 10` drifts to 0.9999999999999999). A
/// non-finite input dominates: `fsum([inf, -inf])` is nan, `fsum([1, inf])`
/// is inf.
fn math_fsum(args: &[Value]) -> EvalResult {
    let items = crate::eval::control_flow::iterate_value(need_arg("fsum", args, 0)?)?;
    let mut partials: Vec<f64> = Vec::new();
    // `special_sum` accumulates every non-finite input; `inf_sum` only the
    // infinities, so `+inf` and `-inf` together make `inf_sum` NaN — the signal
    // CPython uses to raise "-inf + inf in fsum".
    let mut special_sum = 0.0_f64;
    let mut inf_sum = 0.0_f64;
    for item in items {
        let Some(mut x) = item.as_float() else {
            return Err(type_error(format!("must be real number, not {}", item.type_name())));
        };
        let xsave = x;
        let mut i = 0;
        for j in 0..partials.len() {
            let mut y = partials[j];
            if x.abs() < y.abs() {
                std::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            let lo = y - (hi - x);
            if lo != 0.0 {
                partials[i] = lo;
                i += 1;
            }
            x = hi;
        }
        partials.truncate(i);
        if x != 0.0 {
            if x.is_finite() {
                partials.push(x);
            } else if xsave.is_finite() {
                // A finite input whose running partial overflowed to infinity.
                return Err(overflow_error("intermediate overflow in fsum"));
            } else {
                if xsave.is_infinite() {
                    inf_sum += xsave;
                }
                special_sum += xsave;
                partials.clear();
            }
        }
    }
    if special_sum != 0.0 {
        if inf_sum.is_nan() {
            return Err(value_error("-inf + inf in fsum"));
        }
        return Ok(Value::Float(special_sum));
    }
    // Fold from +0.0 (not `Iterator::sum`, whose empty identity is -0.0) so an
    // empty or fully-cancelling sum returns +0.0 as CPython does.
    Ok(Value::Float(partials.iter().copied().fold(0.0_f64, |a, b| a + b)))
}

/// `math.dist(p, q)` — Euclidean distance between two equal-length point
/// sequences.
/// `nextafter(x, y)` via IEEE-754 bit stepping (avoids the MSRV-gated
/// `f64::next_up`/`next_down`). Moves one representable step from `x` toward
/// `y`.
fn next_after(x: f64, y: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    if x == y {
        return y;
    }
    if x == 0.0 {
        // The smallest subnormal, with the sign of the target direction.
        return f64::from_bits(1).copysign(y);
    }
    let bits = x.to_bits();
    // Incrementing the raw bit pattern increases magnitude; whether that heads
    // toward `y` depends on `x`'s sign.
    let next = if (x < y) == (x > 0.0) { bits + 1 } else { bits - 1 };
    f64::from_bits(next)
}

fn math_dist(args: &[Value]) -> EvalResult {
    let p = crate::eval::control_flow::iterate_value(need_arg("dist", args, 0)?)?;
    let q = crate::eval::control_flow::iterate_value(need_arg("dist", args, 1)?)?;
    if p.len() != q.len() {
        return Err(value_error("both points must have the same number of dimensions"));
    }
    let mut sum = 0.0_f64;
    for (a, b) in p.iter().zip(q.iter()) {
        let (Some(a), Some(b)) = (a.as_float(), b.as_float()) else {
            return Err(type_error("coordinates must be numbers"));
        };
        let d = a - b;
        sum += d * d;
    }
    Ok(Value::Float(sum.sqrt()))
}

/// `math.lcm(*integers)` — least common multiple; `lcm() == 1`, any zero → 0.
fn math_lcm(args: &[Value]) -> EvalResult {
    use num_bigint::BigInt;
    use num_traits::{Signed as _, Zero as _};
    let mut acc = BigInt::from(1);
    for arg in args {
        let n = crate::value::value_as_bigint(arg).ok_or_else(|| {
            type_error(format!("'{}' object cannot be interpreted as an integer", arg.type_name()))
        })?;
        if acc.is_zero() || n.is_zero() {
            acc = BigInt::from(0);
            continue;
        }
        let g = num_integer::Integer::gcd(&acc, &n);
        acc = (&acc / &g * &n).abs();
    }
    Ok(crate::value::int_from_bigint(acc))
}

/// `math.isclose(a, b, *, rel_tol=1e-09, abs_tol=0.0)`.
fn math_isclose(
    func: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    let a = arg_f64(func, args, 0)?;
    let b = arg_f64(func, args, 1)?;
    let rel_tol = kwargs.get("rel_tol").and_then(Value::as_float).unwrap_or(1e-9);
    let abs_tol = kwargs.get("abs_tol").and_then(Value::as_float).unwrap_or(0.0);
    if rel_tol < 0.0 || abs_tol < 0.0 {
        return Err(value_error("tolerances must be non-negative"));
    }
    let close = if a == b {
        true
    } else if a.is_infinite() || b.is_infinite() {
        false
    } else {
        let diff = (a - b).abs();
        diff <= (rel_tol * b.abs()).max(rel_tol * a.abs()) || diff <= abs_tol
    };
    Ok(Value::Bool(close))
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
    use num_bigint::BigInt;
    // CPython returns an arbitrary-precision integer (`factorial(30)` is a
    // 33-digit number), so accumulate in BigInt and normalize back to a
    // machine int when it fits — consistent with `math.perm`/`math.comb`.
    let n = match arg {
        Value::Int(i) => BigInt::from(*i),
        Value::Bool(b) => BigInt::from(i64::from(*b)),
        Value::BigInt(b) => (**b).clone(),
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
    if n.sign() == num_bigint::Sign::Minus {
        return Err(value_error("factorial() not defined for negative values"));
    }
    let mut result = BigInt::from(1u32);
    let mut k = BigInt::from(2u32);
    while k <= n {
        result *= &k;
        k += 1u32;
    }
    Ok(crate::value::int_from_bigint(result))
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
        kwargs: &indexmap::IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        call(func, args, kwargs)
    }
}

// `math.erf` / `math.erfc`, ported operation-for-operation from CPython's
// `mathmodule.c` (`m_erf`/`m_erfc`) so results match bit-for-bit: a power series
// for |x| < 1.5 and a continued fraction for erfc otherwise.
const ERF_SERIES_CUTOFF: f64 = 1.5;
const ERF_SERIES_NTERMS: usize = 25;
const ERFC_CONTFRAC_CUTOFF: f64 = 30.0;
const ERFC_CONTFRAC_TERMS: usize = 50;
const SQRTPI: f64 = 1.772_453_850_905_516_027_298_167_483_341_145_182_8;

fn erf_series(x: f64) -> f64 {
    let x2 = x * x;
    let mut acc = 0.0;
    let mut fk = ERF_SERIES_NTERMS as f64 + 0.5;
    for _ in 0..ERF_SERIES_NTERMS {
        acc = 2.0 + x2 * acc / fk;
        fk -= 1.0;
    }
    acc * x * (-x2).exp() / SQRTPI
}

fn erfc_contfrac(x: f64) -> f64 {
    if x >= ERFC_CONTFRAC_CUTOFF {
        return 0.0;
    }
    let x2 = x * x;
    let mut a = 0.0;
    let mut da = 0.5;
    let (mut p, mut p_last) = (1.0, 0.0);
    let mut q = da + x2;
    let mut q_last = 1.0;
    for _ in 0..ERFC_CONTFRAC_TERMS {
        a += da;
        da += 2.0;
        let b = da + x2;
        let temp_p = p;
        p = b * p - a * p_last;
        p_last = temp_p;
        let temp_q = q;
        q = b * q - a * q_last;
        q_last = temp_q;
    }
    p / q * x * (-x2).exp() / SQRTPI
}

fn erf(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let absx = x.abs();
    if absx < ERF_SERIES_CUTOFF {
        erf_series(x)
    } else {
        let cf = erfc_contfrac(absx);
        if x > 0.0 { 1.0 - cf } else { cf - 1.0 }
    }
}

fn erfc(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let absx = x.abs();
    if absx < ERF_SERIES_CUTOFF {
        1.0 - erf_series(x)
    } else {
        let cf = erfc_contfrac(absx);
        if x > 0.0 { cf } else { 2.0 - cf }
    }
}

/// `math.gamma` via the Lanczos approximation (g=7, n=9). Accurate to ~15
/// significant digits across the range the sandbox sees, and exact enough that
/// integer arguments round-trip (`gamma(5) == 24.0` to display precision).
fn gamma(x: f64) -> f64 {
    // Lanczos coefficients for g = 7.
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        // Reflection formula for the left half-plane.
        std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma(1.0 - x))
    } else {
        let x = x - 1.0;
        let mut a = C[0];
        let t = x + G + 0.5;
        for (i, &c) in C.iter().enumerate().skip(1) {
            a += c / (x + i as f64);
        }
        (2.0 * std::f64::consts::PI).sqrt() * t.powf(x + 0.5) * (-t).exp() * a
    }
}
