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
    eval::{eval_expr, functions::resolve_proxy},
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
    // Bound async expression nesting (non-numeric chains like `"a"+"a"+…`) so a
    // pathological chain raises RecursionError and stops growing the host stack
    // rather than recursing to the op-count limit.
    state.enter_expr().map_err(EvalError::Interpreter)?;
    let out = eval_binop_inner(state, node, tools).await;
    state.exit_expr();
    out
}

async fn eval_binop_inner(
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
    // tight numeric loops. On overflow, promote to BigInt rather than
    // raising OverflowError (CPython arbitrary-precision ints).
    if let (Value::Int(a), Value::Int(b)) = (&left, &right) {
        match node.op {
            ast::Operator::Add => {
                return Ok(match a.checked_add(*b) {
                    Some(v) => Value::Int(v),
                    None => crate::value::int_from_bigint(
                        num_bigint::BigInt::from(*a) + num_bigint::BigInt::from(*b),
                    ),
                });
            }
            ast::Operator::Sub => {
                return Ok(match a.checked_sub(*b) {
                    Some(v) => Value::Int(v),
                    None => crate::value::int_from_bigint(
                        num_bigint::BigInt::from(*a) - num_bigint::BigInt::from(*b),
                    ),
                });
            }
            ast::Operator::Mult => {
                return Ok(match a.checked_mul(*b) {
                    Some(v) => Value::Int(v),
                    None => crate::value::int_from_bigint(
                        num_bigint::BigInt::from(*a) * num_bigint::BigInt::from(*b),
                    ),
                });
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
pub fn apply_binop(
    left: &Value,
    right: &Value,
    op: ast::Operator,
    decimal_prec: i64,
    max_int_bits: u64,
) -> Result<Value, EvalError> {
    // `dict_keys` / `dict_items` are set-like: coerce them to a set so
    // `d.keys() & other`, `|`, `-`, `^` reuse the set arithmetic.
    // `dict_values` is NOT set-like, so it is left as-is and a set op on
    // it falls through to the normal TypeError.
    let left_set = dictview_as_set(left);
    let right_set = dictview_as_set(right);
    let left = left_set.as_ref().unwrap_or(left);
    let right = right_set.as_ref().unwrap_or(right);
    match op {
        ast::Operator::Add => {
            crate::types::dispatch_binop(crate::types::BinOp::Add, left, right, decimal_prec)
        }
        ast::Operator::Sub => {
            crate::types::dispatch_binop(crate::types::BinOp::Sub, left, right, decimal_prec)
        }
        ast::Operator::Mult => {
            crate::types::dispatch_binop(crate::types::BinOp::Mul, left, right, decimal_prec)
        }
        ast::Operator::Div => {
            crate::types::dispatch_binop(crate::types::BinOp::Div, left, right, decimal_prec)
        }
        ast::Operator::FloorDiv => {
            crate::types::dispatch_binop(crate::types::BinOp::FloorDiv, left, right, decimal_prec)
        }
        ast::Operator::Mod => {
            crate::types::dispatch_binop(crate::types::BinOp::Mod, left, right, decimal_prec)
        }
        ast::Operator::Pow => {
            // Integer / float `**` honours the configured `max_int_bits` (like
            // the shifts below); Decimal/Fraction/enum pow routes through the
            // type arith slots.
            if matches!(left, Value::Int(_) | Value::BigInt(_) | Value::Bool(_) | Value::Float(_))
                && matches!(
                    right,
                    Value::Int(_) | Value::BigInt(_) | Value::Bool(_) | Value::Float(_)
                )
            {
                pow_values(left, right, max_int_bits)
            } else {
                crate::types::dispatch_binop(crate::types::BinOp::Pow, left, right, decimal_prec)
            }
        }
        ast::Operator::LShift => lshift_values(left, right, max_int_bits),
        ast::Operator::RShift => rshift_values(left, right, max_int_bits),
        ast::Operator::BitOr => bitor_values(left, right),
        ast::Operator::BitXor => bitxor_values(left, right),
        ast::Operator::BitAnd => bitand_values(left, right),
        ast::Operator::MatMult => matmult_values(left, right),
    }
}

/// Coerce a set-like dict view (`dict_keys` / `dict_items`) into a
/// `Value::Set` of its elements for set arithmetic. `dict_values` is not
/// set-like (values may be unhashable / duplicated), so returns `None`.
fn dictview_as_set(value: &Value) -> Option<Value> {
    let Value::DictView { dict, kind } = value else { return None };
    let guard = dict.lock();
    let items: Vec<Value> = match kind {
        crate::value::DictViewKind::Keys => {
            guard.keys().map(crate::value::ValueKey::to_value).collect()
        }
        crate::value::DictViewKind::Items => {
            guard.iter().map(|(k, v)| Value::Tuple(vec![k.to_value(), v.clone()])).collect()
        }
        crate::value::DictViewKind::Values => return None,
    };
    Some(Value::Set(crate::value::shared_set(crate::pyset::SetBody::from_items(items))))
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
        // The sync builtin path has no config in scope; use the default cap.
        // The configured cap is applied on the async `apply_binop` int/float path.
        crate::types::BinOp::Pow => pow_values(left, right, 1_048_576),
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
        Value::BigInt(b) => {
            use num_traits::ToPrimitive as _;
            b.to_f64().ok_or_else(|| {
                EvalError::Exception(crate::value::ExceptionValue::new(
                    "OverflowError",
                    "int too large to convert to float",
                ))
            })
        }
        Value::Float(f) => Ok(*f),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        _ => Err(InterpreterError::TypeError(format!(
            "unsupported operand type for numeric operation: '{}'",
            v.type_name()
        ))
        .into()),
    }
}

/// Coerce a Value to an i64 (fails if BigInt is out of range).
fn to_int(v: &Value) -> Result<i64, EvalError> {
    crate::value::value_as_i64(v).ok_or_else(|| {
        if matches!(v, Value::BigInt(_)) {
            EvalError::Exception(crate::value::ExceptionValue::new(
                "OverflowError",
                "Python int too large to convert to C long",
            ))
        } else {
            InterpreterError::TypeError(format!(
                "unsupported operand type for integer operation: '{}'",
                v.type_name()
            ))
            .into()
        }
    })
}

/// Coerce a Value to BigInt for arbitrary-precision arithmetic.
fn to_bigint(v: &Value) -> Result<num_bigint::BigInt, EvalError> {
    crate::value::value_as_bigint(v).ok_or_else(|| {
        InterpreterError::TypeError(format!(
            "unsupported operand type for integer operation: '{}'",
            v.type_name()
        ))
        .into()
    })
}

fn int_add(left: &Value, right: &Value) -> Result<Value, EvalError> {
    if let (Value::Int(a), Value::Int(b)) = (left, right) {
        if let Some(v) = a.checked_add(*b) {
            return Ok(Value::Int(v));
        }
    }
    Ok(crate::value::int_from_bigint(to_bigint(left)? + to_bigint(right)?))
}

fn int_sub(left: &Value, right: &Value) -> Result<Value, EvalError> {
    if let (Value::Int(a), Value::Int(b)) = (left, right) {
        if let Some(v) = a.checked_sub(*b) {
            return Ok(Value::Int(v));
        }
    }
    Ok(crate::value::int_from_bigint(to_bigint(left)? - to_bigint(right)?))
}

fn int_mul(left: &Value, right: &Value) -> Result<Value, EvalError> {
    if let (Value::Int(a), Value::Int(b)) = (left, right) {
        if let Some(v) = a.checked_mul(*b) {
            return Ok(Value::Int(v));
        }
    }
    Ok(crate::value::int_from_bigint(to_bigint(left)? * to_bigint(right)?))
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
        // bytearray concatenation — `bytearray + (bytes|bytearray)` yields a
        // new bytearray (CPython). `bytes + bytearray` yields bytes.
        (Value::ByteArray(a), Value::Bytes(b)) => {
            let mut result = a.lock().clone();
            result.extend_from_slice(b);
            Ok(Value::ByteArray(crate::value::shared_bytes(result)))
        }
        (Value::ByteArray(a), Value::ByteArray(b)) => {
            let mut result = a.lock().clone();
            result.extend_from_slice(&b.lock());
            Ok(Value::ByteArray(crate::value::shared_bytes(result)))
        }
        (Value::Bytes(a), Value::ByteArray(b)) => {
            let mut result = a.clone();
            result.extend_from_slice(&b.lock());
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
                int_add(left, right)
            }
        }
    }
}

/// The set body of a `set`/`frozenset` (a snapshot clone); `None` for anything
/// else. Lets the set-algebra operators accept either concrete type and run the
/// order-faithful table algebra (CPython presize/merge) rather than a rebuild.
fn set_like_body(v: &Value) -> Option<crate::pyset::SetBody> {
    match v {
        Value::Set(s) => Some(s.lock().clone()),
        Value::Frozenset(f) => Some((**f).clone()),
        _ => None,
    }
}

/// Wrap set-algebra output in the LEFT operand's concrete type — CPython's
/// `a OP b` returns `type(a)`, so `frozenset - set` is a frozenset while
/// `set - frozenset` is a plain set.
fn wrap_set_body(left: &Value, body: crate::pyset::SetBody) -> Value {
    if matches!(left, Value::Frozenset(_)) {
        Value::Frozenset(std::sync::Arc::new(body))
    } else {
        Value::Set(crate::value::shared_set(body))
    }
}

fn sub_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    // Set difference
    if let (Some(a), Some(b)) = (set_like_body(left), set_like_body(right)) {
        return Ok(wrap_set_body(left, a.difference_with(&b)));
    }

    if either_is_float(left, right) {
        Ok(Value::Float(to_float(left)? - to_float(right)?))
    } else {
        int_sub(left, right)
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
        // bytearray * int (repeat) -> a new bytearray.
        (Value::ByteArray(b), _) if matches!(right, Value::Int(_) | Value::Bool(_)) => {
            let n = to_int(right)?;
            let src = b.lock();
            if n <= 0 {
                return Ok(Value::ByteArray(crate::value::shared_bytes(Vec::new())));
            }
            let result_size = src.len().saturating_mul(repeat_count(n));
            if result_size > MAX_STRING_SIZE {
                return Err(InterpreterError::LimitExceeded(format!(
                    "bytes repetition would create {result_size} bytes (limit: {MAX_STRING_SIZE})"
                ))
                .into());
            }
            Ok(Value::ByteArray(crate::value::shared_bytes(src.repeat(repeat_count(n)))))
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
                int_mul(left, right)
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
        // Fast path stays in i64; large or overflowing operands promote to
        // BigInt so `10**30 // 7` matches CPython's unbounded integers.
        if let (Value::Int(a), Value::Int(b)) = (left, right) {
            if *b == 0 {
                return Err(crate::value::ExceptionValue::zero_division_error(
                    "integer division or modulo by zero",
                )
                .into());
            }
            // `i64::MIN / -1` is the sole overflowing case; it promotes below.
            if !(*a == i64::MIN && *b == -1) {
                return Ok(Value::Int(python_floordiv(*a, *b)));
            }
        }
        use num_integer::Integer as _;
        use num_traits::Zero as _;
        let l = to_bigint(left)?;
        let r = to_bigint(right)?;
        if r.is_zero() {
            return Err(crate::value::ExceptionValue::zero_division_error(
                "integer division or modulo by zero",
            )
            .into());
        }
        // BigInt::div_floor rounds towards negative infinity, as Python does.
        Ok(crate::value::int_from_bigint(l.div_floor(&r)))
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
    // `bytes % args` / `bytearray % args` — printf-style bytes formatting.
    match left {
        Value::Bytes(template) => {
            return crate::eval::strings::bytes_percent_format(template, right);
        }
        Value::ByteArray(template) => {
            let snapshot = template.lock().clone();
            return crate::eval::strings::bytes_percent_format(&snapshot, right).map(|v| match v {
                // bytearray % args yields a bytearray.
                Value::Bytes(b) => Value::ByteArray(crate::value::shared_bytes(b)),
                other => other,
            });
        }
        _ => {}
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
        // Fast path in i64; promote to BigInt for large/overflowing operands.
        if let (Value::Int(a), Value::Int(b)) = (left, right) {
            if *b == 0 {
                return Err(crate::value::ExceptionValue::zero_division_error(
                    "integer division or modulo by zero",
                )
                .into());
            }
            // `i64::MIN % -1` overflows in debug; route it through BigInt.
            if !(*a == i64::MIN && *b == -1) {
                return Ok(Value::Int(python_mod(*a, *b)));
            }
        }
        use num_integer::Integer as _;
        use num_traits::Zero as _;
        let l = to_bigint(left)?;
        let r = to_bigint(right)?;
        if r.is_zero() {
            return Err(crate::value::ExceptionValue::zero_division_error(
                "integer division or modulo by zero",
            )
            .into());
        }
        // BigInt::mod_floor gives a remainder with the divisor's sign, as Python.
        Ok(crate::value::int_from_bigint(l.mod_floor(&r)))
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
    // Snapshot both operands' outer rows first (releasing each lock) so `m @ m`
    // — the same Arc on both sides — does not lock the one mutex twice (deadlock).
    let a_guard = a.lock().clone();
    let b_guard = b.lock().clone();
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
        for col in 0..n {
            let mut sum = 0.0;
            for (ai, brow) in row.iter().zip(b_rows.iter()) {
                sum += ai * brow[col];
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

fn pow_values(left: &Value, right: &Value, max_int_bits: u64) -> Result<Value, EvalError> {
    if either_is_float(left, right) {
        let l = to_float(left)?;
        let r = to_float(right)?;
        // 0 to a negative power is a division by zero in CPython.
        if l == 0.0 && r < 0.0 {
            return Err(crate::value::ExceptionValue::zero_division_error(
                "0.0 cannot be raised to a negative power",
            )
            .into());
        }
        // A negative base raised to a non-integer power yields a complex number
        // (the principal branch), as CPython does — Rust's powf gives NaN. Use
        // CPython's `_Py_c_pow` real-base formula (len*cos/sin via atan2) rather
        // than num_complex's ln/exp so the last ULPs match CPython byte-for-byte.
        if l < 0.0 && r.fract() != 0.0 {
            let len = l.abs().powf(r);
            let phase = 0.0f64.atan2(l) * r;
            let c = num_complex::Complex64::new(len * phase.cos(), len * phase.sin());
            return Ok(Value::Complex(Box::new(c)));
        }
        Ok(Value::Float(l.powf(r)))
    } else {
        let l = crate::value::value_as_bigint(left).ok_or_else(|| {
            InterpreterError::TypeError(format!(
                "unsupported operand type(s) for **: '{}' and '{}'",
                left.type_name(),
                right.type_name()
            ))
        })?;
        let r = crate::value::value_as_bigint(right).ok_or_else(|| {
            InterpreterError::TypeError(format!(
                "unsupported operand type(s) for **: '{}' and '{}'",
                left.type_name(),
                right.type_name()
            ))
        })?;
        use num_traits::{Pow, ToPrimitive as _, Zero as _};
        if r < num_bigint::BigInt::from(0) {
            // `0 ** -n` is a division by zero (matches the float path above).
            if l.is_zero() {
                return Err(crate::value::ExceptionValue::zero_division_error(
                    "0.0 cannot be raised to a negative power",
                )
                .into());
            }
            let l_f = l.to_f64().unwrap_or(f64::INFINITY);
            let r_f = r.to_f64().unwrap_or(f64::NEG_INFINITY);
            Ok(Value::Float(l_f.powf(r_f)))
        } else if r.is_zero() {
            Ok(Value::Int(1))
        } else {
            let exp = u32::try_from(&r).map_err(|_| {
                EvalError::Exception(crate::value::ExceptionValue::new(
                    "OverflowError",
                    "exponent too large for integer power",
                ))
            })?;
            // Cap absurd exponents that would OOM (security).
            if exp > 1_000_000 {
                return Err(EvalError::Exception(crate::value::ExceptionValue::new(
                    "OverflowError",
                    "exponent too large for integer power",
                )));
            }
            crate::value::int_from_bigint_limited(l.pow(exp), max_int_bits)
        }
    }
}

fn lshift_values(left: &Value, right: &Value, max_int_bits: u64) -> Result<Value, EvalError> {
    use num_traits::{Signed, ToPrimitive as _};
    let l = to_bigint(left)?;
    let r = to_bigint(right)?;
    if r.is_negative() {
        return Err(InterpreterError::ValueError("negative shift count".into()).into());
    }
    // Cap absurd shifts (security / memory).
    let shift = r.to_u32().ok_or_else(|| {
        EvalError::Exception(crate::value::ExceptionValue::new(
            "OverflowError",
            "shift count too large",
        ))
    })?;
    if shift > 1_000_000 {
        return Err(EvalError::Exception(crate::value::ExceptionValue::new(
            "OverflowError",
            "shift count too large",
        )));
    }
    crate::value::int_from_bigint_limited(l << shift, max_int_bits)
}

fn rshift_values(left: &Value, right: &Value, max_int_bits: u64) -> Result<Value, EvalError> {
    use num_traits::{Signed, ToPrimitive as _};
    let l = to_bigint(left)?;
    let r = to_bigint(right)?;
    if r.is_negative() {
        return Err(InterpreterError::ValueError("negative shift count".into()).into());
    }
    let shift = r.to_u32().ok_or_else(|| {
        EvalError::Exception(crate::value::ExceptionValue::new(
            "OverflowError",
            "shift count too large",
        ))
    })?;
    crate::value::int_from_bigint_limited(l >> shift, max_int_bits)
}

fn bitor_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    // Counter union — multiset combine taking max of counts per key.
    // Matches CPython's `Counter.__or__` (`Counter(_keep_positive)` filter).
    if let (Value::Counter(a), Value::Counter(b)) = (left, right) {
        return Ok(Value::Counter(crate::types::counter_combine_op(a, b, std::cmp::Ord::max)));
    }
    // Set union
    if let (Some(a), Some(b)) = (set_like_body(left), set_like_body(right)) {
        return Ok(wrap_set_body(left, a.union_with(&b)));
    }
    // Dict merge (Python 3.9+). Accepts dict/OrderedDict on either side. The
    // result is an OrderedDict when either operand is one: `od | d` goes
    // through OrderedDict.__or__, and `d | od` through OrderedDict.__ror__
    // (the right operand is a dict subclass), both yielding OrderedDict.
    if let (Some(a), Some(b)) = (left.as_dict(), right.as_dict()) {
        let mut result = a.lock().clone();
        for (k, v) in b.lock().iter() {
            result.insert(k.clone(), v.clone());
        }
        let merged = crate::value::shared_dict(result);
        return Ok(
            if matches!(left, Value::OrderedDict(_)) || matches!(right, Value::OrderedDict(_)) {
                Value::OrderedDict(merged)
            } else {
                Value::Dict(merged)
            },
        );
    }
    // `bool | bool` stays a bool in CPython; a mixed bool/int is an int.
    if let (Value::Bool(a), Value::Bool(b)) = (left, right) {
        return Ok(Value::Bool(*a | *b));
    }
    if let (Value::Int(a), Value::Int(b)) = (left, right) {
        return Ok(Value::Int(a | b));
    }
    Ok(crate::value::int_from_bigint(to_bigint(left)? | to_bigint(right)?))
}

fn bitxor_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    // Set symmetric difference
    if let (Some(a), Some(b)) = (set_like_body(left), set_like_body(right)) {
        return Ok(wrap_set_body(left, a.symmetric_difference_with(&b)));
    }
    if let (Value::Bool(a), Value::Bool(b)) = (left, right) {
        return Ok(Value::Bool(*a ^ *b));
    }
    if let (Value::Int(a), Value::Int(b)) = (left, right) {
        return Ok(Value::Int(a ^ b));
    }
    Ok(crate::value::int_from_bigint(to_bigint(left)? ^ to_bigint(right)?))
}

fn bitand_values(left: &Value, right: &Value) -> Result<Value, EvalError> {
    // Counter intersection — multiset combine taking min of counts.
    // Matches CPython's `Counter.__and__` (positive results only).
    if let (Value::Counter(a), Value::Counter(b)) = (left, right) {
        return Ok(Value::Counter(crate::types::counter_combine_op(a, b, std::cmp::Ord::min)));
    }
    // Set intersection
    if let (Some(a), Some(b)) = (set_like_body(left), set_like_body(right)) {
        return Ok(wrap_set_body(left, a.intersection_with(&b)));
    }
    if let (Value::Bool(a), Value::Bool(b)) = (left, right) {
        return Ok(Value::Bool(*a & *b));
    }
    if let (Value::Int(a), Value::Int(b)) = (left, right) {
        return Ok(Value::Int(a & b));
    }
    Ok(crate::value::int_from_bigint(to_bigint(left)? & to_bigint(right)?))
}

/// Build the `Counter` for a unary `+`/`-`: keep counts that are strictly
/// positive after the sign is applied (`negate` for `-c`), dropping the rest.
fn counter_unary(
    c: &indexmap::IndexMap<crate::value::ValueKey, Value>,
    negate: bool,
) -> indexmap::IndexMap<crate::value::ValueKey, Value> {
    let mut result = indexmap::IndexMap::new();
    for (key, val) in c {
        let n = crate::value::value_as_i64(val).unwrap_or(0);
        let kept = if negate { -n } else { n };
        if kept > 0 {
            result.insert(key.clone(), Value::Int(kept));
        }
    }
    result
}

/// Evaluate a unary operation (+x, -x, ~x, not x).
pub async fn eval_unaryop(
    state: &mut InterpreterState,
    node: &ast::ExprUnaryOp,
    tools: &Tools,
) -> EvalResult {
    // Bound `not not …` / `- - - …` chains (see eval_binop).
    state.enter_expr().map_err(EvalError::Interpreter)?;
    let out = eval_unaryop_inner(state, node, tools).await;
    state.exit_expr();
    out
}

async fn eval_unaryop_inner(
    state: &mut InterpreterState,
    node: &ast::ExprUnaryOp,
    tools: &Tools,
) -> EvalResult {
    let operand = eval_expr(state, &node.operand, tools).await?;
    let operand = resolve_proxy(&operand).await?;
    // Route through op::unaryop so a user-class operand's __neg__/__pos__/
    // __invert__ dispatches; builtin operands fall through to apply_unaryop.
    crate::eval::op::unaryop(state, node.op, &operand, tools).await
}

/// Apply a unary operator to an already-evaluated operand. Shared by the eval
/// spine and the `operator` module (`operator.neg`/`pos`/`invert`/`not_`).
pub async fn apply_unaryop(
    state: &mut InterpreterState,
    op: ast::UnaryOp,
    operand: &Value,
    tools: &Tools,
) -> EvalResult {
    // An IntEnum / IntFlag member behaves as its underlying int under unary
    // `+`/`-`/`~` (`~P.LOW == -4`), so unwrap before dispatch. A plain Enum /
    // Flag is not numeric and keeps its EnumMember form, hitting the TypeError
    // arms below exactly as CPython raises.
    let operand = match operand {
        Value::EnumMember {
            value,
            kind: crate::value::EnumKind::Int | crate::value::EnumKind::IntFlag,
            ..
        } => value.as_ref(),
        other => other,
    };
    match op {
        // Unary `+` is identity on every numeric type (`bool` promotes to int).
        ast::UnaryOp::UAdd => match operand {
            Value::Int(_)
            | Value::BigInt(_)
            | Value::Float(_)
            | Value::Complex(_)
            | Value::Decimal(..)
            | Value::Fraction(_) => Ok(operand.clone()),
            Value::Bool(b) => Ok(Value::Int(i64::from(*b))),
            // `+timedelta` returns the same timedelta.
            Value::TimeDelta(_) => Ok(operand.clone()),
            // `+Counter` keeps only the strictly-positive counts (CPython's
            // `Counter.__pos__`, used to strip zero/negative tallies).
            Value::Counter(c) => Ok(Value::Counter(counter_unary(c, false))),
            _ => Err(InterpreterError::TypeError(format!(
                "bad operand type for unary +: '{}'",
                operand.type_name()
            ))
            .into()),
        },
        ast::UnaryOp::USub => match operand {
            // `checked_neg` handles i64::MIN, whose negation overflows i64:
            // promote to BigInt instead of wrapping (release) / panicking (debug).
            Value::Int(i) => Ok(i.checked_neg().map_or_else(
                || crate::value::int_from_bigint(-num_bigint::BigInt::from(*i)),
                Value::Int,
            )),
            Value::BigInt(b) => Ok(crate::value::int_from_bigint(-(*b.clone()))),
            Value::Float(f) => Ok(Value::Float(-*f)),
            Value::Complex(c) => Ok(Value::Complex(Box::new(-(**c)))),
            Value::Bool(b) => Ok(Value::Int(if *b { -1 } else { 0 })),
            // Unary `-` is the arithmetic negate (`__neg__`), which applies the
            // context and yields *positive* zero for any zero operand
            // (`-Decimal('0')` and `-Decimal('-0.0')` both print `0`) — unlike
            // `copy_negate`. So the result is never neg-zero.
            Value::Decimal(d, _) => Ok(Value::Decimal(Box::new(-(*d.clone())), false)),
            Value::Fraction(fr) => Ok(Value::Fraction(Box::new(-(*fr.clone())))),
            // `-Counter` negates every count and keeps the now-positive ones
            // (CPython's `Counter.__neg__`).
            Value::Counter(c) => Ok(Value::Counter(counter_unary(c, true))),
            // `-timedelta` negates the microsecond total.
            Value::TimeDelta(us) => Ok(Value::TimeDelta(-*us)),
            _ => Err(InterpreterError::TypeError(format!(
                "bad operand type for unary -: '{}'",
                operand.type_name()
            ))
            .into()),
        },
        ast::UnaryOp::Not => {
            let cond = match crate::eval::op::try_truthy_sync(operand) {
                Some(b) => b,
                None => crate::eval::op::truthy(state, operand, tools).await?,
            };
            Ok(Value::Bool(!cond))
        }
        // `~x == -x - 1`. Keep the i64 fast path; fall to BigInt for anything
        // outside it so `~(2**70)` works instead of raising OverflowError.
        ast::UnaryOp::Invert => match operand {
            Value::Int(i) => Ok(Value::Int(!*i)),
            Value::Bool(b) => Ok(Value::Int(!i64::from(*b))),
            Value::BigInt(_) => {
                let n = to_bigint(operand)?;
                Ok(crate::value::int_from_bigint(-n - 1))
            }
            _ => Err(InterpreterError::TypeError(format!(
                "bad operand type for unary ~: '{}'",
                operand.type_name()
            ))
            .into()),
        },
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
    let result = match op {
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
            // The equality half of `<=` must be the SAME equality as `==` —
            // `dispatch_eq`, which covers BigInt/Decimal/Fraction/etc. The old
            // `values_equal` here was a second, incomplete table with no arm for
            // any of those, so `x <= x` was False for every value outside i64.
            Ok(lt || eq_via_dispatch(state, left, right)?)
        }
        ast::CmpOp::Gt => crate::types::dispatch_lt(right, left),
        ast::CmpOp::GtE => {
            let gt = crate::types::dispatch_lt(right, left)?;
            Ok(gt || eq_via_dispatch(state, left, right)?)
        }
        ast::CmpOp::Is | ast::CmpOp::IsNot | ast::CmpOp::In | ast::CmpOp::NotIn => {
            unreachable!("identity/membership ops handled at eval_compare before reaching here")
        }
    }?;
    // The structural walk (`recurse_eq`/`values_equal`) hit the recursion limit
    // — a cyclic or pathologically deep comparison. Its sync `bool` return can't
    // raise, so it set a flag; surface it as `RecursionError` here, matching
    // CPython's `Py_EnterRecursiveCall`.
    if crate::cycle::take_eq_overflow() {
        return Err(InterpreterError::RecursionLimitExceeded {
            limit: crate::cycle::EQ_RECURSION_LIMIT,
        }
        .into());
    }
    Ok(result)
}

/// Equality as `==` computes it, for the equality half of `<=` / `>=`.
///
/// Routes through `crate::types::dispatch_eq` (the same entry `==` uses) rather
/// than the local `values_equal` table, so wide-numeric and stdlib value types
/// (`BigInt`, `Decimal`, `Fraction`, `Date`, ...) compare correctly.
fn eq_via_dispatch(
    state: &InterpreterState,
    left: &Value,
    right: &Value,
) -> Result<bool, EvalError> {
    let Value::Bool(b) = crate::types::dispatch_eq(state, left, right)? else {
        unreachable!("dispatch_eq always returns Value::Bool");
    };
    Ok(b)
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
    // Bound the structural recursion (shares the depth counter with
    // `recurse_eq`) so distinct cyclic containers stop rather than overflow the
    // stack; a same-object cycle short-circuits via `Arc::ptr_eq` below.
    let Some(_depth) = crate::cycle::eq_depth_enter() else {
        return false;
    };
    match (left, right) {
        (Value::None, Value::None) => true,
        // Singletons: identical to themselves, distinct from everything else.
        (Value::Ellipsis, Value::Ellipsis) | (Value::NotImplemented, Value::NotImplemented) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Bytes(a), Value::Bytes(b)) => a == b,
        (Value::ByteArray(a), Value::ByteArray(b)) => {
            std::sync::Arc::ptr_eq(a, b) || *a.lock() == *b.lock()
        }
        (Value::ByteArray(a), Value::Bytes(b)) | (Value::Bytes(b), Value::ByteArray(a)) => {
            *a.lock() == *b
        }
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
            // Snapshot and release the locks before recursing — `values_equal`
            // re-locks these lists on a self-reference, which would deadlock if
            // the guard were still held.
            let a_guard = a.lock().clone();
            let b_guard = b.lock().clone();
            a_guard.len() == b_guard.len()
                && a_guard.iter().zip(b_guard.iter()).all(|(x, y)| values_equal(x, y))
        }
        (Value::Array { items: a, .. }, Value::Array { items: b, .. }) => {
            if std::sync::Arc::ptr_eq(a, b) {
                return true;
            }
            let a_guard = a.lock().clone();
            let b_guard = b.lock().clone();
            a_guard.len() == b_guard.len()
                && a_guard.iter().zip(b_guard.iter()).all(|(x, y)| values_equal(x, y))
        }
        // Deques compare element-wise in order; `maxlen` is not part of equality.
        (Value::Deque { items: a, .. }, Value::Deque { items: b, .. }) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal(x, y))
        }
        (Value::Tuple(a), Value::Tuple(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal(x, y))
        }
        // Two OrderedDicts compare order-sensitively (CPython OrderedDict.__eq__).
        (Value::OrderedDict(a), Value::OrderedDict(b)) => {
            if std::sync::Arc::ptr_eq(a, b) {
                return true;
            }
            let a = a.lock().clone();
            let b = b.lock().clone();
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|((ka, va), (kb, vb))| ka == kb && values_equal(va, vb))
        }
        // Plain dict, or a mixed dict/OrderedDict pair — unordered comparison.
        (Value::Dict(a) | Value::OrderedDict(a), Value::Dict(b) | Value::OrderedDict(b)) => {
            if std::sync::Arc::ptr_eq(a, b) {
                return true;
            }
            let a = a.lock().clone();
            let b = b.lock().clone();
            if a.len() != b.len() {
                return false;
            }
            a.iter().all(|(k, v)| b.get(k).is_some_and(|bv| values_equal(v, bv)))
        }
        // set/frozenset equality is order-independent and cross-type:
        // CPython's `{1, 2} == frozenset([2, 1])` is True.
        (Value::Set(_) | Value::Frozenset(_), Value::Set(_) | Value::Frozenset(_)) => {
            let (Some(a), Some(b)) = (left.set_items(), right.set_items()) else {
                return false;
            };
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
        // CPython here — tracked by gap-instance-dict-key-equality-dunder-parity.
        (Value::Instance(a), Value::Instance(b)) => {
            if a.class_name != b.class_name {
                return false;
            }
            // Shared storage: same Arc ⇒ identity equal (like list).
            if std::sync::Arc::ptr_eq(&a.fields, &b.fields) {
                return true;
            }
            // Snapshot and release before recursing — a field may reference the
            // instance (cycle), and re-locking under the held guard deadlocks.
            let af = a.fields.lock().clone();
            let bf = b.fields.lock().clone();
            if af.len() != bf.len() {
                return false;
            }
            af.iter().all(|(name, va)| bf.get(name).is_some_and(|vb| values_equal(va, vb)))
        }
        // CPython caches compiled patterns, so `re.compile(p) == re.compile(p)`
        // is True (same cached object). We model that by comparing sources.
        (Value::RePattern(a), Value::RePattern(b)) => a == b,
        // Two slices are equal iff their start/stop/step all match.
        (Value::Slice(a), Value::Slice(b)) => {
            values_equal(&a.start, &b.start)
                && values_equal(&a.stop, &b.stop)
                && values_equal(&a.step, &b.step)
        }
        // Types / classes / exception types are singletons keyed by name, so
        // two handles for the same name are identical (`ValueError is
        // ValueError`, which `values_is` routes here). The variant a name
        // resolves to (Type vs ExceptionType) is context-dependent, so a
        // cross-variant name match still counts.
        // A bare builtin type/function name is a `BuiltinName`, while `type(x)`
        // yields a `Type` — so `int is int`, `type(42) is int`, `int == int`,
        // and `len is len` all match here by name across the variants.
        (
            Value::ExceptionType(a) | Value::Type(a) | Value::Class(a) | Value::BuiltinName(a),
            Value::ExceptionType(b) | Value::Type(b) | Value::Class(b) | Value::BuiltinName(b),
        ) => a == b,
        // Temporal types: value-based equality (missing arms previously fell to
        // `_ => false`, so `date == date`, `[date] == [date]`, and `date in
        // [...]` were all False). datetime equality compares the wall clock for
        // naive pairs and the absolute instant for aware pairs; a naive value
        // never equals an aware one.
        (Value::Date(a), Value::Date(b)) => a == b,
        (Value::Time(a), Value::Time(b)) => a == b,
        (Value::TimeDelta(a), Value::TimeDelta(b)) => a == b,
        (
            Value::DateTime { dt: a, tz_offset_secs: ta },
            Value::DateTime { dt: b, tz_offset_secs: tb },
        ) => match (ta, tb) {
            (None, None) => a == b,
            (Some(oa), Some(ob)) => {
                (*a - chrono::Duration::seconds(i64::from(*oa)))
                    == (*b - chrono::Duration::seconds(i64::from(*ob)))
            }
            _ => false,
        },
        // Two timezones are equal when their UTC offsets match (CPython compares
        // the offset; our model carries no name to distinguish otherwise).
        (Value::TimeZone(a), Value::TimeZone(b)) => a == b,
        // Ranges are equal when they yield the same sequence, not when their
        // fields match: `range(0, 3, 2) == range(0, 4, 2)` is True (both [0, 2]).
        // CPython: equal length, and — when non-empty — equal first element and
        // (when length > 1) equal step.
        (
            Value::Range { start: sa, stop: pa, step: ta },
            Value::Range { start: sb, stop: pb, step: tb },
        ) => {
            let la = crate::types::range_length(*sa, *pa, *ta);
            la == crate::types::range_length(*sb, *pb, *tb)
                && (la == 0 || (sa == sb && (la == 1 || ta == tb)))
        }
        _ => false,
    }
}

/// Check value identity (Python `is`).
///
/// The single source of truth for `is` — the sync numeric fast path
/// (`eval::try_numeric_compare`) routes here too, so `1 is 1` and `[] is []`
/// can no longer disagree.
///
/// Identity for the reference types we back with a shared `Arc` (list, instance,
/// function, lambda, lru_cache) is real pointer identity: an alias `is` its
/// source, and two freshly-built objects are not. For immutable value types our
/// clone-on-load model cannot distinguish "same object" from "equal object", and
/// CPython caches/interns most of them anyway, so we fall back to equality —
/// which matches CPython for the stable cases (small ints, bools, `None`, short
/// interned strings). Uncached immutables (a large int, a long non-interned
/// string) are the documented divergence.
pub(crate) fn values_is(left: &Value, right: &Value) -> bool {
    use std::sync::Arc;
    match (left, right) {
        (Value::List(a), Value::List(b)) => Arc::ptr_eq(a, b),
        (Value::Instance(a), Value::Instance(b)) => Arc::ptr_eq(&a.fields, &b.fields),
        (Value::Function(a), Value::Function(b)) => Arc::ptr_eq(a, b),
        (Value::Lambda(a), Value::Lambda(b)) => Arc::ptr_eq(a, b),
        (Value::LruCache(a), Value::LruCache(b)) => Arc::ptr_eq(a, b),
        // Sets/frozensets are reference types: `s is s` (and an alias) is True,
        // but two separately-built equal sets are distinct objects.
        (Value::Set(a), Value::Set(b)) => Arc::ptr_eq(a, b),
        (Value::Frozenset(a), Value::Frozenset(b)) => Arc::ptr_eq(a, b),
        // dict/bytearray/array are Arc-backed mutable reference types too: an
        // alias `is` its source, two separately-built equal objects are not.
        (Value::Dict(a), Value::Dict(b)) => Arc::ptr_eq(a, b),
        (Value::OrderedDict(a), Value::OrderedDict(b)) => Arc::ptr_eq(a, b),
        (Value::ByteArray(a), Value::ByteArray(b)) => Arc::ptr_eq(a, b),
        (Value::Array { items: a, .. }, Value::Array { items: b, .. }) => Arc::ptr_eq(a, b),
        // Iterator objects are identified by the id/cursor keying their state
        // in the interpreter, so a generator/lazy/builtin iterator is identical
        // exactly to itself (`g is g`, `iter(g) is g`).
        (Value::Generator { id: a }, Value::Generator { id: b }) => a == b,
        (Value::Lazy { cursor_id: a, .. }, Value::Lazy { cursor_id: b, .. }) => a == b,
        (Value::BuiltinIter { id: a, .. }, Value::BuiltinIter { id: b, .. }) => a == b,
        // A reference type is never identical to a value of any other type.
        (
            Value::List(_)
            | Value::Instance(_)
            | Value::Function(_)
            | Value::Lambda(_)
            | Value::LruCache(_)
            | Value::Generator { .. }
            | Value::Lazy { .. }
            | Value::BuiltinIter { .. }
            | Value::Set(_)
            | Value::Frozenset(_)
            | Value::Dict(_)
            | Value::OrderedDict(_)
            | Value::ByteArray(_)
            | Value::Array { .. },
            _,
        )
        | (
            _,
            Value::List(_)
            | Value::Instance(_)
            | Value::Function(_)
            | Value::Lambda(_)
            | Value::LruCache(_)
            | Value::Generator { .. }
            | Value::Lazy { .. }
            | Value::BuiltinIter { .. }
            | Value::Set(_)
            | Value::Frozenset(_)
            | Value::Dict(_)
            | Value::OrderedDict(_)
            | Value::ByteArray(_)
            | Value::Array { .. },
        ) => false,
        // Immutable value types: `is` is identity, which we approximate by
        // equality for same-typed immutables (`3 is 3`, interned-string-style
        // `"a" is "a"`). The only values `values_equal` unifies across
        // *different* Python types are the numeric tower (`1 == 1.0 == True`),
        // but those are distinct objects — `3.0 is 3`, `1 is True`, `1 is 1.0`
        // are all False in CPython — so a numeric-kind mismatch short-circuits
        // to False. (Type objects have several representations — `int` as a
        // BuiltinName vs `type(42)` — that legitimately `is`-match, so the
        // guard is scoped to numbers, not a blanket discriminant check.)
        _ => {
            if let (Some(a), Some(b)) = (numeric_is_kind(left), numeric_is_kind(right)) {
                if a != b {
                    return false;
                }
            }
            values_equal(left, right)
        }
    }
}

/// A tag identifying the Python numeric *type* of a value for `is`
/// identity, or `None` for non-numbers. `int`/`float`/`bool`/`complex` are
/// distinct object types even though they compare equal across the tower,
/// so `is` must not unify them.
fn numeric_is_kind(v: &Value) -> Option<u8> {
    match v {
        Value::Bool(_) => Some(0),
        Value::Int(_) | Value::BigInt(_) => Some(1),
        Value::Float(_) => Some(2),
        Value::Complex(_) => Some(3),
        _ => None,
    }
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
    // Bound nested-ternary chains `x if p else x if p else …` (see eval_binop).
    state.enter_expr().map_err(EvalError::Interpreter)?;
    let out = eval_ifexp_inner(state, node, tools).await;
    state.exit_expr();
    out
}

async fn eval_ifexp_inner(
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
