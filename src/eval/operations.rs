// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

#![expect(
    clippy::cast_precision_loss,
    reason = "Python's numeric tower coerces Int -> Float for mixed arithmetic and for certain \
              integer ops (negative exponents, very large exponents, int->float shifts at the \
              boundary); these coercions lose precision above 2^53, which matches CPython's \
              `float(int)` behavior exactly — the loss is the specified semantic. Scoping \
              the allow to this arithmetic module keeps it from sliding elsewhere"
)]
#![expect(
    clippy::float_cmp,
    reason = "Python's `==` on floats uses exact bit equality (with IEEE NaN oddity); \
              `3.0 == 3.0` is a supported Python operation and users rely on it. We cannot \
              fold float comparisons into an epsilon-based check without changing visible \
              language semantics"
)]

use rustpython_parser::ast;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{eval_expr, functions::resolve_proxy, literals::value_to_key},
    state::InterpreterState,
    tools::Tools,
    value::{Value, shared_list},
};

/// Maximum number of elements in a collection created by multiplication.
const MAX_COLLECTION_SIZE: usize = 10_000_000;
/// Maximum string size in bytes from multiplication.
const MAX_STRING_SIZE: usize = 100 * 1024 * 1024;

/// Convert a validated-positive i64 repeat count into a `usize` for
/// container replication. Saturates to `usize::MAX` on 32-bit platforms
/// where the count might exceed usize; downstream `MAX_COLLECTION_SIZE`
/// / `MAX_STRING_SIZE` checks guard against oversized results.
fn repeat_count(n: i64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

/// Convert a shift/exponent `i64` that has been range-checked to `[0, 64)`
/// into a `u32`. The range check guarantees the conversion succeeds; the
/// `try_from` keeps the invariant explicit rather than a silent `as` cast.
fn bounded_u32(n: i64) -> Result<u32, EvalError> {
    u32::try_from(n).map_err(|_| {
        InterpreterError::Runtime(
            "shift/exponent count out of u32 range (internal invariant)".into(),
        )
        .into()
    })
}

/// Evaluate a binary operation (`a + b`, `a * b`, etc.).
///
/// Both operand expressions evaluate and lazy-resolve their proxies
/// before dispatch hits `op::binop`, which owns the user-class slot
/// lookup (forward + reflected) and the fallthrough to the sync
/// `apply_binop` builtin kernel.
pub async fn eval_binop(
    state: &mut InterpreterState,
    node: &ast::ExprBinOp,
    tools: &Tools,
) -> EvalResult {
    let left = match crate::eval::try_eval_expr_sync(state, &node.left, tools) {
        Some(r) => r?,
        None => eval_expr(state, &node.left, tools).await?,
    };
    let left = resolve_proxy(&left).await?;
    let right = match crate::eval::try_eval_expr_sync(state, &node.right, tools) {
        Some(r) => r?,
        None => eval_expr(state, &node.right, tools).await?,
    };
    let right = resolve_proxy(&right).await?;

    // Int+Int / Int-Int / Int*Int fast path — the dominant case in
    // tight numeric loops (`total + i * 3`, `total - 1`, etc.).
    // Skips the full `op::binop` dispatch chain (Instance slot
    // lookup, reflected dunder fallback, type-object slot dispatch)
    // for the case where both operands are concrete `Value::Int`.
    // Falls through to the general path on overflow so the existing
    // OverflowError is raised — `security_integer_overflow_detected`
    // covers this contract.
    if let (Value::Int(a), Value::Int(b)) = (&left, &right) {
        match node.op {
            ast::Operator::Add => {
                if let Some(v) = a.checked_add(*b) {
                    return Ok(Value::Int(v));
                }
            }
            ast::Operator::Sub => {
                if let Some(v) = a.checked_sub(*b) {
                    return Ok(Value::Int(v));
                }
            }
            ast::Operator::Mult => {
                if let Some(v) = a.checked_mul(*b) {
                    return Ok(Value::Int(v));
                }
            }
            _ => {}
        }
    }

    crate::eval::op::binop(state, node.op, &left, &right, tools).await
}

/// Apply a binary operator to two builtin values. Shared by
/// `eval_binop` (after the Instance slot fast-path) and augmented
/// assignment.
///
/// The 7 main arithmetic ops route through the type-object slot table
/// (`types::dispatch_binop`); the type-object's `arith_slot` decides
/// which builtin pair it accepts and calls back into
/// [`apply_binop_builtin`] for the actual integer/float work.
///
/// Bitwise/shift/matmul stay on the direct dispatch path — they're
/// int-only on builtins, so the cross-type slot-table shape doesn't
/// buy anything. `MatMult` (`@`) is intentionally unsupported on
/// builtins; user classes override it via `__matmul__` at the
/// `eval_binop` entry.
pub fn apply_binop(left: &Value, right: &Value, op: ast::Operator) -> Result<Value, EvalError> {
    match op {
        ast::Operator::Add => crate::types::dispatch_binop(crate::types::BinOp::Add, left, right),
        ast::Operator::Sub => crate::types::dispatch_binop(crate::types::BinOp::Sub, left, right),
        ast::Operator::Mult => crate::types::dispatch_binop(crate::types::BinOp::Mul, left, right),
        ast::Operator::Div => crate::types::dispatch_binop(crate::types::BinOp::Div, left, right),
        ast::Operator::FloorDiv => {
            crate::types::dispatch_binop(crate::types::BinOp::FloorDiv, left, right)
        }
        ast::Operator::Mod => crate::types::dispatch_binop(crate::types::BinOp::Mod, left, right),
        ast::Operator::Pow => crate::types::dispatch_binop(crate::types::BinOp::Pow, left, right),
        ast::Operator::LShift => lshift_values(left, right),
        ast::Operator::RShift => rshift_values(left, right),
        ast::Operator::BitOr => bitor_values(left, right),
        ast::Operator::BitXor => bitxor_values(left, right),
        ast::Operator::BitAnd => bitand_values(left, right),
        ast::Operator::MatMult => matmult_values(left, right),
    }
}

/// Builtin-pair arithmetic kernel for the type-object slot table.
///
/// The slot table in `types.rs` matches on the (lhs, rhs) type pair
/// and decides which arithmetic kernel to call. Every accepted pair
/// routes here, where the per-operator `add_values` / `sub_values` /
/// etc. handle the actual numeric work. Pure-Rust; no async, no
/// dispatch — the caller (the slot) has already decided this is a
/// pair we know how to compute.
///
/// IntEnum / StrEnum members unwrap to their underlying numeric or
/// string value before the kernel runs (matching CPython, where
/// arithmetic on IntEnum is arithmetic on the underlying int). Plain
/// `Enum` members don't unwrap — they raise TypeError per CPython.
pub fn apply_binop_builtin(
    op: crate::types::BinOp,
    left: &Value,
    right: &Value,
) -> Result<Value, EvalError> {
    let left_unwrapped = unwrap_enum_for_arith(left);
    let right_unwrapped = unwrap_enum_for_arith(right);
    if !std::ptr::eq(left_unwrapped, left) || !std::ptr::eq(right_unwrapped, right) {
        return apply_binop_builtin(op, left_unwrapped, right_unwrapped);
    }
    match op {
        crate::types::BinOp::Add => add_values(left, right),
        crate::types::BinOp::Sub => sub_values(left, right),
        crate::types::BinOp::Mul => mult_values(left, right),
        crate::types::BinOp::Div => div_values(left, right),
        crate::types::BinOp::FloorDiv => floordiv_values(left, right),
        crate::types::BinOp::Mod => mod_values(left, right),
        crate::types::BinOp::Pow => pow_values(left, right),
    }
}

/// Unwrap an EnumMember to its underlying value when its kind is
/// Int or Str (IntEnum / StrEnum behave as their underlying type for
/// arithmetic). Plain Enum members are returned as-is so the arith
/// dispatcher raises TypeError for them.
fn unwrap_enum_for_arith(value: &Value) -> &Value {
    match value {
        Value::EnumMember {
            value: inner,
            kind: crate::value::EnumKind::Int | crate::value::EnumKind::Str,
            ..
        } => inner.as_ref(),
        _ => value,
    }
}

/// Coerce a Value to an f64 for arithmetic.
fn to_float(v: &Value) -> Result<f64, EvalError> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        _ => Err(InterpreterError::TypeError(format!(
            "unsupported operand type for numeric operation: '{}'",
            v.type_name()
        ))
        .into()),
    }
}

/// Coerce a Value to an i64.
fn to_int(v: &Value) -> Result<i64, EvalError> {
    match v {
        Value::Int(i) => Ok(*i),
        Value::Bool(b) => Ok(i64::from(*b)),
        _ => Err(InterpreterError::TypeError(format!(
            "unsupported operand type for integer operation: '{}'",
            v.type_name()
        ))
        .into()),
    }
}

/// Check if either operand is float (requiring float arithmetic).
const fn either_is_float(left: &Value, right: &Value) -> bool {
    matches!(left, Value::Float(_)) || matches!(right, Value::Float(_))
}

fn add_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    match (left, right) {
        // String concatenation
        (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{a}{b}").into())),
        // Bytes concatenation
        (Value::Bytes(a), Value::Bytes(b)) => {
            let mut result = a.clone();
            result.extend_from_slice(b);
            Ok(Value::Bytes(result))
        }
        // List concatenation. Lists are shared via Arc<Mutex<Vec>>;
        // snapshot both under their locks and emit a fresh shared list.
        (Value::List(a), Value::List(b)) => {
            let a_snapshot = a.lock().clone();
            let b_snapshot = b.lock().clone();
            let mut result = Vec::with_capacity(a_snapshot.len() + b_snapshot.len());
            result.extend(a_snapshot);
            result.extend(b_snapshot);
            Ok(Value::List(shared_list(result)))
        }
        // Tuple concatenation
        (Value::Tuple(a), Value::Tuple(b)) => {
            let mut result = a.clone();
            result.extend(b.iter().cloned());
            Ok(Value::Tuple(result))
        }
        // Numeric addition
        _ => {
            if either_is_float(left, right) {
                Ok(Value::Float(to_float(left)? + to_float(right)?))
            } else {
                let l = to_int(left)?;
                let r = to_int(right)?;
                Ok(Value::Int(l.checked_add(r).ok_or_else(|| {
                    EvalError::from(InterpreterError::Runtime("integer overflow".into()))
                })?))
            }
        }
    }
}

fn sub_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    // Set difference
    if let (Value::Set(a), Value::Set(b)) = (left, right) {
        let b_keys: Vec<_> = b.iter().filter_map(|v| value_to_key(v).ok()).collect();
        let result: Vec<Value> = a
            .iter()
            .filter(|v| value_to_key(v).map_or(true, |k| !b_keys.contains(&k)))
            .cloned()
            .collect();
        return Ok(Value::Set(result));
    }

    if either_is_float(left, right) {
        Ok(Value::Float(to_float(left)? - to_float(right)?))
    } else {
        let l = to_int(left)?;
        let r = to_int(right)?;
        Ok(Value::Int(l.checked_sub(r).ok_or_else(|| {
            EvalError::from(InterpreterError::Runtime("integer overflow".into()))
        })?))
    }
}

fn mult_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    match (left, right) {
        // String * int (repeat)
        (Value::String(s), _) if matches!(right, Value::Int(_) | Value::Bool(_)) => {
            let n = to_int(right)?;
            if n <= 0 {
                return Ok(Value::String("".into()));
            }
            let result_size = s.len().saturating_mul(repeat_count(n));
            if result_size > MAX_STRING_SIZE {
                return Err(InterpreterError::LimitExceeded(format!(
                    "string repetition would create {result_size} bytes (limit: {MAX_STRING_SIZE})"
                ))
                .into());
            }
            Ok(Value::String(s.repeat(repeat_count(n))))
        }
        (Value::Int(_) | Value::Bool(_), Value::String(s)) => {
            let n = to_int(left)?;
            if n <= 0 {
                return Ok(Value::String("".into()));
            }
            let result_size = s.len().saturating_mul(repeat_count(n));
            if result_size > MAX_STRING_SIZE {
                return Err(InterpreterError::LimitExceeded(format!(
                    "string repetition would create {result_size} bytes (limit: {MAX_STRING_SIZE})"
                ))
                .into());
            }
            Ok(Value::String(s.repeat(repeat_count(n))))
        }
        // Bytes * int (repeat)
        (Value::Bytes(b), _) if matches!(right, Value::Int(_) | Value::Bool(_)) => {
            let n = to_int(right)?;
            if n <= 0 {
                return Ok(Value::Bytes(Vec::new()));
            }
            let result_size = b.len().saturating_mul(repeat_count(n));
            if result_size > MAX_STRING_SIZE {
                return Err(InterpreterError::LimitExceeded(format!(
                    "bytes repetition would create {result_size} bytes (limit: {MAX_STRING_SIZE})"
                ))
                .into());
            }
            Ok(Value::Bytes(b.repeat(repeat_count(n))))
        }
        (Value::Int(_) | Value::Bool(_), Value::Bytes(b)) => {
            let n = to_int(left)?;
            if n <= 0 {
                return Ok(Value::Bytes(Vec::new()));
            }
            let result_size = b.len().saturating_mul(repeat_count(n));
            if result_size > MAX_STRING_SIZE {
                return Err(InterpreterError::LimitExceeded(format!(
                    "bytes repetition would create {result_size} bytes (limit: {MAX_STRING_SIZE})"
                ))
                .into());
            }
            Ok(Value::Bytes(b.repeat(repeat_count(n))))
        }
        // List * int (repeat). Snapshot the inner Vec under the lock,
        // then build a fresh shared list.
        (Value::List(items), _) if matches!(right, Value::Int(_) | Value::Bool(_)) => {
            let n = to_int(right)?;
            if n <= 0 {
                return Ok(Value::List(shared_list(Vec::new())));
            }
            let snapshot = items.lock().clone();
            let result_size = snapshot.len().saturating_mul(repeat_count(n));
            if result_size > MAX_COLLECTION_SIZE {
                return Err(InterpreterError::LimitExceeded(format!(
                    "list repetition would create {result_size} elements (limit: {MAX_COLLECTION_SIZE})"
                )).into());
            }
            let mut result = Vec::with_capacity(result_size);
            for _ in 0..n {
                result.extend(snapshot.iter().cloned());
            }
            Ok(Value::List(shared_list(result)))
        }
        (_, Value::List(items)) if matches!(left, Value::Int(_) | Value::Bool(_)) => {
            let n = to_int(left)?;
            if n <= 0 {
                return Ok(Value::List(shared_list(Vec::new())));
            }
            let snapshot = items.lock().clone();
            let result_size = snapshot.len().saturating_mul(repeat_count(n));
            if result_size > MAX_COLLECTION_SIZE {
                return Err(InterpreterError::LimitExceeded(format!(
                    "list repetition would create {result_size} elements (limit: {MAX_COLLECTION_SIZE})"
                )).into());
            }
            let mut result = Vec::with_capacity(result_size);
            for _ in 0..n {
                result.extend(snapshot.iter().cloned());
            }
            Ok(Value::List(shared_list(result)))
        }
        // Tuple * int (repeat)
        (Value::Tuple(items), _) if matches!(right, Value::Int(_) | Value::Bool(_)) => {
            let n = to_int(right)?;
            if n <= 0 {
                return Ok(Value::Tuple(Vec::new()));
            }
            let result_size = items.len().saturating_mul(repeat_count(n));
            if result_size > MAX_COLLECTION_SIZE {
                return Err(InterpreterError::LimitExceeded(format!(
                    "tuple repetition would create {result_size} elements (limit: {MAX_COLLECTION_SIZE})"
                )).into());
            }
            let mut result = Vec::with_capacity(result_size);
            for _ in 0..n {
                result.extend(items.iter().cloned());
            }
            Ok(Value::Tuple(result))
        }
        _ => {
            if either_is_float(left, right) {
                Ok(Value::Float(to_float(left)? * to_float(right)?))
            } else {
                let l = to_int(left)?;
                let r = to_int(right)?;
                Ok(Value::Int(l.checked_mul(r).ok_or_else(|| {
                    EvalError::from(InterpreterError::Runtime("integer overflow".into()))
                })?))
            }
        }
    }
}

fn div_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    // Python: / always returns float
    let l = to_float(left)?;
    let r = to_float(right)?;
    if r == 0.0 {
        return Err(crate::value::ExceptionValue::zero_division_error("division by zero").into());
    }
    Ok(Value::Float(l / r))
}

fn floordiv_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    if either_is_float(left, right) {
        let l = to_float(left)?;
        let r = to_float(right)?;
        if r == 0.0 {
            return Err(
                crate::value::ExceptionValue::zero_division_error("division by zero").into()
            );
        }
        Ok(Value::Float((l / r).floor()))
    } else {
        let l = to_int(left)?;
        let r = to_int(right)?;
        if r == 0 {
            return Err(crate::value::ExceptionValue::zero_division_error(
                "integer division or modulo by zero",
            )
            .into());
        }
        // Python floor division rounds towards negative infinity
        Ok(Value::Int(python_floordiv(l, r)))
    }
}

/// Python-style floor division (rounds towards negative infinity).
const fn python_floordiv(a: i64, b: i64) -> i64 {
    let d = a / b;
    let r = a % b;
    if (r != 0) && ((r ^ b) < 0) { d - 1 } else { d }
}

fn mod_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    // `template % args` is printf-style string formatting in Python, not a
    // security risk — the interpreter has no I/O for a format string to reach.
    if let Value::String(template) = left {
        return crate::eval::strings::str_percent_format(template, right);
    }

    if either_is_float(left, right) {
        let l = to_float(left)?;
        let r = to_float(right)?;
        if r == 0.0 {
            return Err(crate::value::ExceptionValue::zero_division_error("modulo by zero").into());
        }
        // Python modulo: result has same sign as divisor
        Ok(Value::Float(r.mul_add(-(l / r).floor(), l)))
    } else {
        let l = to_int(left)?;
        let r = to_int(right)?;
        if r == 0 {
            return Err(crate::value::ExceptionValue::zero_division_error(
                "integer division or modulo by zero",
            )
            .into());
        }
        Ok(Value::Int(python_mod(l, r)))
    }
}

/// Python-style modulo (result has same sign as divisor).
const fn python_mod(a: i64, b: i64) -> i64 {
    let r = a % b;
    if (r != 0) && ((r ^ b) < 0) { r + b } else { r }
}

/// 2-D list matrix multiply (`@`). Rows×cols of ints/floats; result is float
/// when either operand has a float, else int when all products fit.
fn matmult_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    let (Value::List(a), Value::List(b)) = (left, right) else {
        return Err(InterpreterError::TypeError(format!(
            "unsupported operand type(s) for @: '{}' and '{}' (see CONFORMANCE.md#unsupported-language-features)",
            left.type_name(),
            right.type_name()
        ))
        .into());
    };
    let a_guard = a.lock();
    let b_guard = b.lock();
    if a_guard.is_empty() || b_guard.is_empty() {
        return Ok(Value::List(shared_list(Vec::new())));
    }
    // Extract rows of a and columns of b as f64 for a uniform product.
    let mut a_rows: Vec<Vec<f64>> = Vec::with_capacity(a_guard.len());
    let mut n_cols_a = None;
    for row in a_guard.iter() {
        let Value::List(cells) = row else {
            return Err(InterpreterError::TypeError(
                "@ requires a list of lists on the left".into(),
            )
            .into());
        };
        let cells = cells.lock();
        if let Some(n) = n_cols_a {
            if cells.len() != n {
                return Err(InterpreterError::ValueError(
                    "matmul: left operand rows must have equal length".into(),
                )
                .into());
            }
        } else {
            n_cols_a = Some(cells.len());
        }
        let mut r = Vec::with_capacity(cells.len());
        for c in cells.iter() {
            r.push(to_float(c)?);
        }
        a_rows.push(r);
    }
    let k = n_cols_a.unwrap_or(0);
    let mut b_rows: Vec<Vec<f64>> = Vec::with_capacity(b_guard.len());
    let mut n_cols_b = None;
    for row in b_guard.iter() {
        let Value::List(cells) = row else {
            return Err(InterpreterError::TypeError(
                "@ requires a list of lists on the right".into(),
            )
            .into());
        };
        let cells = cells.lock();
        if let Some(n) = n_cols_b {
            if cells.len() != n {
                return Err(InterpreterError::ValueError(
                    "matmul: right operand rows must have equal length".into(),
                )
                .into());
            }
        } else {
            n_cols_b = Some(cells.len());
        }
        let mut r = Vec::with_capacity(cells.len());
        for c in cells.iter() {
            r.push(to_float(c)?);
        }
        b_rows.push(r);
    }
    if b_rows.len() != k {
        return Err(InterpreterError::ValueError(format!(
            "matmul: shapes ({},{}) and ({},{}) not aligned",
            a_rows.len(),
            k,
            b_rows.len(),
            n_cols_b.unwrap_or(0)
        ))
        .into());
    }
    let n = n_cols_b.unwrap_or(0);
    let mut out = Vec::with_capacity(a_rows.len());
    for row in &a_rows {
        let mut out_row = Vec::with_capacity(n);
        for j in 0..n {
            let mut sum = 0.0;
            for (t, &ai) in row.iter().enumerate() {
                sum += ai * b_rows[t][j];
            }
            // Prefer int when the sum is an exact integer in i64 range.
            #[allow(clippy::cast_possible_truncation, clippy::float_cmp)]
            let cell = if sum.fract() == 0.0 && sum >= i64::MIN as f64 && sum <= i64::MAX as f64 {
                Value::Int(sum as i64)
            } else {
                Value::Float(sum)
            };
            out_row.push(cell);
        }
        out.push(Value::List(shared_list(out_row)));
    }
    Ok(Value::List(shared_list(out)))
}

fn pow_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    if either_is_float(left, right) {
        let l = to_float(left)?;
        let r = to_float(right)?;
        Ok(Value::Float(l.powf(r)))
    } else {
        let l = to_int(left)?;
        let r = to_int(right)?;
        if r < 0 {
            // Negative exponent => float result (CPython returns float for ints).
            let l_f = l as f64;
            let r_f = r as f64;
            Ok(Value::Float(l_f.powf(r_f)))
        } else if r == 0 {
            Ok(Value::Int(1))
        } else {
            // Exact integer power via BigInt. Narrow to i64 when possible;
            // otherwise OverflowError (no silent f64 precision loss).
            // Full arbitrary-precision `Value::Int` is a separate ticket.
            use num_traits::Pow;
            let exp = u32::try_from(r).map_err(|_| {
                EvalError::Exception(crate::value::ExceptionValue::new(
                    "OverflowError",
                    "exponent too large for integer power",
                ))
            })?;
            let big = num_bigint::BigInt::from(l).pow(exp);
            match i64::try_from(&big) {
                Ok(n) => Ok(Value::Int(n)),
                Err(_) => Err(EvalError::Exception(crate::value::ExceptionValue::new(
                    "OverflowError",
                    "integer power result exceeds i64 range (arbitrary-precision int not yet enabled)",
                ))),
            }
        }
    }
}

fn lshift_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    let l = to_int(left)?;
    let r = to_int(right)?;
    if r < 0 {
        return Err(InterpreterError::ValueError("negative shift count".into()).into());
    }
    if r >= 64 {
        return Ok(Value::Int(0));
    }
    let shift = bounded_u32(r)?;
    Ok(Value::Int(
        l.checked_shl(shift)
            .ok_or_else(|| EvalError::from(InterpreterError::Runtime("integer overflow".into())))?,
    ))
}

fn rshift_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    let l = to_int(left)?;
    let r = to_int(right)?;
    if r < 0 {
        return Err(InterpreterError::ValueError("negative shift count".into()).into());
    }
    if r >= 64 {
        return Ok(Value::Int(if l < 0 { -1 } else { 0 }));
    }
    let shift = bounded_u32(r)?;
    Ok(Value::Int(
        l.checked_shr(shift)
            .ok_or_else(|| EvalError::from(InterpreterError::Runtime("integer overflow".into())))?,
    ))
}

fn bitor_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    // Counter union — multiset combine taking max of counts per key.
    // Matches CPython's `Counter.__or__` (`Counter(_keep_positive)` filter).
    if let (Value::Counter(a), Value::Counter(b)) = (left, right) {
        return Ok(Value::Counter(crate::types::counter_combine_op(a, b, std::cmp::Ord::max)));
    }
    // Set union
    if let (Value::Set(a), Value::Set(b)) = (left, right) {
        let mut result = a.clone();
        for item in b {
            let key = value_to_key(item).ok();
            let already_has = result.iter().any(|r| value_to_key(r).ok() == key);
            if !already_has {
                result.push(item.clone());
            }
        }
        return Ok(Value::Set(result));
    }
    // Dict merge (Python 3.9+)
    if let (Value::Dict(a), Value::Dict(b)) = (left, right) {
        let mut result = a.clone();
        for (k, v) in b {
            result.insert(k.clone(), v.clone());
        }
        return Ok(Value::Dict(result));
    }
    let l = to_int(left)?;
    let r = to_int(right)?;
    Ok(Value::Int(l | r))
}

fn bitxor_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    // Set symmetric difference
    if let (Value::Set(a), Value::Set(b)) = (left, right) {
        let a_keys: Vec<_> = a.iter().filter_map(|v| value_to_key(v).ok()).collect();
        let b_keys: Vec<_> = b.iter().filter_map(|v| value_to_key(v).ok()).collect();
        let mut result: Vec<Value> = a
            .iter()
            .filter(|v| value_to_key(v).map_or(true, |k| !b_keys.contains(&k)))
            .cloned()
            .collect();
        for item in b {
            if let Ok(k) = value_to_key(item) {
                if !a_keys.contains(&k) {
                    result.push(item.clone());
                }
            }
        }
        return Ok(Value::Set(result));
    }
    let l = to_int(left)?;
    let r = to_int(right)?;
    Ok(Value::Int(l ^ r))
}

fn bitand_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    // Counter intersection — multiset combine taking min of counts.
    // Matches CPython's `Counter.__and__` (positive results only).
    if let (Value::Counter(a), Value::Counter(b)) = (left, right) {
        return Ok(Value::Counter(crate::types::counter_combine_op(a, b, std::cmp::Ord::min)));
    }
    // Set intersection
    if let (Value::Set(a), Value::Set(b)) = (left, right) {
        let b_keys: Vec<_> = b.iter().filter_map(|v| value_to_key(v).ok()).collect();
        let result: Vec<Value> = a
            .iter()
            .filter(|v| value_to_key(v).is_ok_and(|k| b_keys.contains(&k)))
            .cloned()
            .collect();
        return Ok(Value::Set(result));
    }
    let l = to_int(left)?;
    let r = to_int(right)?;
    Ok(Value::Int(l & r))
}

/// Evaluate a unary operation (+x, -x, ~x, not x).
pub async fn eval_unaryop(
    state: &mut InterpreterState,
    node: &ast::ExprUnaryOp,
    tools: &Tools,
) -> EvalResult {
    let operand = eval_expr(state, &node.operand, tools).await?;
    let operand = resolve_proxy(&operand).await?;

    match node.op {
        ast::UnaryOp::UAdd => match &operand {
            Value::Int(i) => Ok(Value::Int(*i)),
            Value::Float(f) => Ok(Value::Float(*f)),
            Value::Bool(b) => Ok(Value::Int(i64::from(*b))),
            _ => Err(InterpreterError::TypeError(format!(
                "bad operand type for unary +: '{}'",
                operand.type_name()
            ))
            .into()),
        },
        ast::UnaryOp::USub => match &operand {
            Value::Int(i) => Ok(Value::Int(-*i)),
            Value::Float(f) => Ok(Value::Float(-*f)),
            Value::Bool(b) => Ok(Value::Int(if *b { -1 } else { 0 })),
            _ => Err(InterpreterError::TypeError(format!(
                "bad operand type for unary -: '{}'",
                operand.type_name()
            ))
            .into()),
        },
        ast::UnaryOp::Not => {
            let cond = match crate::eval::op::try_truthy_sync(&operand) {
                Some(b) => b,
                None => crate::eval::op::truthy(state, &operand, tools).await?,
            };
            Ok(Value::Bool(!cond))
        }
        ast::UnaryOp::Invert => {
            let i = to_int(&operand)?;
            Ok(Value::Int(!i))
        }
    }
}

/// Evaluate a comparison operation (a < b, a == b, etc.).
/// Supports chained comparisons (a < b < c).
pub async fn eval_compare(
    state: &mut InterpreterState,
    node: &ast::ExprCompare,
    tools: &Tools,
) -> EvalResult {
    let mut left = match crate::eval::try_eval_expr_sync(state, &node.left, tools) {
        Some(r) => r?,
        None => eval_expr(state, &node.left, tools).await?,
    };
    left = resolve_proxy(&left).await?;
    // Track the LHS variable name when the LHS is a bare Name —
    // user-class __lt__/__eq__/etc. may mutate `self`, and we need
    // to write the post-call value back to the binding.
    let mut left_var: Option<String> = match &*node.left {
        ast::Expr::Name(n) => Some(n.id.as_str().to_string()),
        _ => None,
    };

    for (op, comparator) in node.ops.iter().zip(node.comparators.iter()) {
        let right = match crate::eval::try_eval_expr_sync(state, comparator, tools) {
            Some(r) => r?,
            None => eval_expr(state, comparator, tools).await?,
        };
        let right = resolve_proxy(&right).await?;
        let right_var: Option<String> = match comparator {
            ast::Expr::Name(n) => Some(n.id.as_str().to_string()),
            _ => None,
        };
        let result = match op {
            // Membership routes through op::contains (user-class
            // __contains__ + builtin slot table behind one entry).
            ast::CmpOp::In => crate::eval::op::contains(state, &right, &left, tools).await?,
            ast::CmpOp::NotIn => !crate::eval::op::contains(state, &right, &left, tools).await?,
            // Identity is sync; no slot override.
            ast::CmpOp::Is => values_is(&left, &right),
            ast::CmpOp::IsNot => !values_is(&left, &right),
            // Rich-compare flows through op::compare; write back any
            // post-slot mutation to the originating variable.
            _ => {
                let (cmp, post_left, post_right) =
                    crate::eval::op::compare(state, *op, &left, &right, tools).await?;
                if let (Some(name), Some(v)) = (&left_var, post_left) {
                    state.set_variable(name, v).map_err(EvalError::Interpreter)?;
                }
                if let (Some(name), Some(v)) = (&right_var, post_right) {
                    state.set_variable(name, v).map_err(EvalError::Interpreter)?;
                }
                cmp
            }
        };
        if !result {
            return Ok(Value::Bool(false));
        }
        left = right;
        left_var = right_var;
    }
    Ok(Value::Bool(true))
}

/// Builtin-pair rich-compare kernel — called by `op::compare` for the
/// fall-through path when neither operand is a user-class instance
/// with a matching dunder slot. Maps each `CmpOp` to its sync entry
/// in `crate::types`. `Is`/`IsNot`/`In`/`NotIn` are handled at the
/// eval-layer entry (`eval_compare`) and never reach here.
pub fn compare_builtin(
    state: &InterpreterState,
    op: ast::CmpOp,
    left: &Value,
    right: &Value,
) -> Result<bool, EvalError> {
    match op {
        ast::CmpOp::Eq => {
            let Value::Bool(b) = crate::types::dispatch_eq(state, left, right)? else {
                unreachable!("dispatch_eq always returns Value::Bool");
            };
            Ok(b)
        }
        ast::CmpOp::NotEq => {
            let Value::Bool(b) = crate::types::dispatch_eq(state, left, right)? else {
                unreachable!("dispatch_eq always returns Value::Bool");
            };
            Ok(!b)
        }
        ast::CmpOp::Lt => crate::types::dispatch_lt(left, right),
        ast::CmpOp::LtE => {
            let lt = crate::types::dispatch_lt(left, right)?;
            Ok(lt || values_equal(left, right))
        }
        ast::CmpOp::Gt => crate::types::dispatch_lt(right, left),
        ast::CmpOp::GtE => {
            let gt = crate::types::dispatch_lt(right, left)?;
            Ok(gt || values_equal(left, right))
        }
        ast::CmpOp::Is | ast::CmpOp::IsNot | ast::CmpOp::In | ast::CmpOp::NotIn => {
            unreachable!("identity/membership ops handled at eval_compare before reaching here")
        }
    }
}

/// Public wrapper for value equality (used by other modules).
pub fn values_equal_pub(left: &Value, right: &Value) -> bool {
    values_equal(left, right)
}

/// Public wrapper for less-than comparison (used by sorted/min/max).
pub fn compare_lt(left: &Value, right: &Value) -> Result<bool, EvalError> {
    crate::types::dispatch_lt(left, right)
}

// `op::lt` replaced the previous `compare_lt_async` helper.

/// Check value equality (Python semantics: True == 1, False == 0).
fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::None, Value::None) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Bytes(a), Value::Bytes(b)) => a == b,
        // Cross-type numeric equality (Python: True == 1, 1 == 1.0)
        (Value::Bool(b), Value::Int(i)) | (Value::Int(i), Value::Bool(b)) => *i == i64::from(*b),
        (Value::Bool(b), Value::Float(f)) | (Value::Float(f), Value::Bool(b)) => {
            *f == if *b { 1.0 } else { 0.0 }
        }
        (Value::Int(i), Value::Float(f)) | (Value::Float(f), Value::Int(i)) => *f == (*i as f64),
        // Collection equality — List and Tuple compare element-wise.
        // List is shared via Arc<Mutex<Vec>> so identity-aliased pairs
        // short-circuit via Arc::ptr_eq before any locking, then both
        // sides lock for the element walk.
        (Value::List(a), Value::List(b)) => {
            if std::sync::Arc::ptr_eq(a, b) {
                return true;
            }
            let a_guard = a.lock();
            let b_guard = b.lock();
            a_guard.len() == b_guard.len()
                && a_guard.iter().zip(b_guard.iter()).all(|(x, y)| values_equal(x, y))
        }
        (Value::Tuple(a), Value::Tuple(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal(x, y))
        }
        (Value::Dict(a), Value::Dict(b)) => {
            if a.len() != b.len() {
                return false;
            }
            a.iter().all(|(k, v)| b.get(k).is_some_and(|bv| values_equal(v, bv)))
        }
        (Value::Set(a), Value::Set(b)) => {
            if a.len() != b.len() {
                return false;
            }
            // Every element in a must be in b
            a.iter().all(|av| b.iter().any(|bv| values_equal(av, bv)))
        }
        // EnumMember equality: identity (class + member name) when
        // both are enum members; value-based when one side is an
        // IntEnum / StrEnum and the other is a raw int/str.
        (
            Value::EnumMember { class_name: c1, member_name: m1, .. },
            Value::EnumMember { class_name: c2, member_name: m2, .. },
        ) => c1 == c2 && m1 == m2,
        (
            Value::EnumMember {
                value,
                kind: crate::value::EnumKind::Int | crate::value::EnumKind::Str,
                ..
            },
            other,
        ) => values_equal(value.as_ref(), other),
        (
            other,
            Value::EnumMember {
                value,
                kind: crate::value::EnumKind::Int | crate::value::EnumKind::Str,
                ..
            },
        ) => values_equal(other, value.as_ref()),
        // User-class instance structural equality: same class and the
        // intersection of fields all compare equal. This is the sync
        // fallback the ValueKey::Instance dict-key equality relies on
        // (the async __eq__ slot can't run here). Classes whose __eq__
        // diverges from structural equality (e.g. case-insensitive
        // string wrappers) will see dict/set semantics drift from
        // CPython here — a tracked limitation.
        (Value::Instance(a), Value::Instance(b)) => {
            if a.class_name != b.class_name {
                return false;
            }
            // Shared storage: same Arc ⇒ identity equal (like list).
            if std::sync::Arc::ptr_eq(&a.fields, &b.fields) {
                return true;
            }
            let af = a.fields.lock();
            let bf = b.fields.lock();
            if af.len() != bf.len() {
                return false;
            }
            af.iter().all(|(name, va)| bf.get(name).is_some_and(|vb| values_equal(va, vb)))
        }
        _ => false,
    }
}

/// Check value identity (Python `is`). In our interpreter, only None has identity.
const fn values_is(left: &Value, right: &Value) -> bool {
    matches!((left, right), (Value::None, Value::None))
}

/// Evaluate a boolean operation (and, or) with short-circuit, returning actual values.
pub async fn eval_boolop(
    state: &mut InterpreterState,
    node: &ast::ExprBoolOp,
    tools: &Tools,
) -> EvalResult {
    match node.op {
        ast::BoolOp::And => {
            let mut last = Value::Bool(true);
            for value_node in &node.values {
                last = eval_expr(state, value_node, tools).await?;
                last = resolve_proxy(&last).await?;
                let cond = match crate::eval::op::try_truthy_sync(&last) {
                    Some(b) => b,
                    None => crate::eval::op::truthy(state, &last, tools).await?,
                };
                if !cond {
                    return Ok(last);
                }
            }
            Ok(last)
        }
        ast::BoolOp::Or => {
            let mut last = Value::Bool(false);
            for value_node in &node.values {
                last = eval_expr(state, value_node, tools).await?;
                last = resolve_proxy(&last).await?;
                let cond = match crate::eval::op::try_truthy_sync(&last) {
                    Some(b) => b,
                    None => crate::eval::op::truthy(state, &last, tools).await?,
                };
                if cond {
                    return Ok(last);
                }
            }
            Ok(last)
        }
    }
}

/// Evaluate a conditional expression (ternary: x if cond else y).
pub async fn eval_ifexp(
    state: &mut InterpreterState,
    node: &ast::ExprIfExp,
    tools: &Tools,
) -> EvalResult {
    let test = eval_expr(state, &node.test, tools).await?;
    let test = resolve_proxy(&test).await?;
    let cond = match crate::eval::op::try_truthy_sync(&test) {
        Some(b) => b,
        None => crate::eval::op::truthy(state, &test, tools).await?,
    };
    if cond {
        eval_expr(state, &node.body, tools).await
    } else {
        eval_expr(state, &node.orelse, tools).await
    }
}
