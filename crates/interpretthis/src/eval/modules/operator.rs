// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `operator` module — the functional forms of the
//! built-in operators (`operator.add(a, b)` == `a + b`), commonly paired with
//! `functools.reduce`, `itertools.accumulate`, and `sorted`/`min`/`max` keys.
//!
//! Binary/unary/comparison functions route through the same async `op::` layer
//! the eval spine uses, so a user class's `__add__` / `__lt__` / `__neg__` runs.
//! The callable-returning members (`itemgetter`, `attrgetter`, `methodcaller`)
//! are handled separately as they yield a callable value.

use indexmap::IndexMap;
use rustpython_parser::ast::{CmpOp, Operator, UnaryOp};

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    tools::Tools,
    value::Value,
};

/// Whether `operator` provides a callable named `name`.
pub fn has_function(name: &str) -> bool {
    binary_operator(name).is_some()
        || compare_op(name).is_some()
        || matches!(
            name,
            "neg"
                | "pos"
                | "invert"
                | "not_"
                | "truth"
                | "abs"
                | "index"
                | "is_"
                | "is_not"
                | "contains"
                | "getitem"
                | "concat"
                | "countOf"
                | "indexOf"
                | "itemgetter"
                | "attrgetter"
                | "methodcaller"
        )
}

/// Map an `operator` name to the AST binary operator it performs.
fn binary_operator(name: &str) -> Option<Operator> {
    Some(match name {
        "add" | "iadd" => Operator::Add,
        "sub" | "isub" => Operator::Sub,
        "mul" | "imul" => Operator::Mult,
        "matmul" | "imatmul" => Operator::MatMult,
        "truediv" | "itruediv" => Operator::Div,
        "floordiv" | "ifloordiv" => Operator::FloorDiv,
        "mod" | "imod" => Operator::Mod,
        "pow" | "ipow" => Operator::Pow,
        "lshift" | "ilshift" => Operator::LShift,
        "rshift" | "irshift" => Operator::RShift,
        "and_" | "iand" => Operator::BitAnd,
        "or_" | "ior" => Operator::BitOr,
        "xor" | "ixor" => Operator::BitXor,
        _ => return None,
    })
}

/// Map an `operator` name to its comparison operator.
fn compare_op(name: &str) -> Option<CmpOp> {
    Some(match name {
        "lt" => CmpOp::Lt,
        "le" => CmpOp::LtE,
        "eq" => CmpOp::Eq,
        "ne" => CmpOp::NotEq,
        "gt" => CmpOp::Gt,
        "ge" => CmpOp::GtE,
        _ => return None,
    })
}

fn arg2<'a>(func: &str, args: &'a [Value]) -> Result<(&'a Value, &'a Value), EvalError> {
    match args {
        [a, b] => Ok((a, b)),
        _ => Err(InterpreterError::TypeError(format!(
            "{func} expected 2 arguments, got {}",
            args.len()
        ))
        .into()),
    }
}

fn arg1<'a>(func: &str, args: &'a [Value]) -> Result<&'a Value, EvalError> {
    match args {
        [a] => Ok(a),
        _ => Err(InterpreterError::TypeError(format!(
            "{func} expected 1 argument, got {}",
            args.len()
        ))
        .into()),
    }
}

/// `operator` module registration.
pub struct OperatorModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for OperatorModule {
    fn name(&self) -> &'static str {
        "operator"
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        state: &mut InterpreterState,
        func: &str,
        args: &[Value],
        kwargs: &IndexMap<String, Value>,
        tools: &Tools,
    ) -> EvalResult {
        // Callable factories return a captured getter applied later.
        match func {
            "itemgetter" => {
                if args.is_empty() {
                    return Err(InterpreterError::TypeError(
                        "itemgetter expected 1 argument, got 0".into(),
                    )
                    .into());
                }
                return Ok(Value::OperatorGetter(Box::new(
                    crate::value::OperatorGetter::ItemGetter(args.to_vec()),
                )));
            }
            "attrgetter" => {
                if args.is_empty() {
                    return Err(InterpreterError::TypeError(
                        "attrgetter expected 1 argument, got 0".into(),
                    )
                    .into());
                }
                let mut attrs = Vec::with_capacity(args.len());
                for a in args {
                    let Value::String(s) = a else {
                        return Err(InterpreterError::TypeError(
                            "attribute name must be a string".into(),
                        )
                        .into());
                    };
                    attrs.push(s.split('.').map(str::to_string).collect());
                }
                return Ok(Value::OperatorGetter(Box::new(
                    crate::value::OperatorGetter::AttrGetter(attrs),
                )));
            }
            "methodcaller" => {
                let Some(Value::String(name)) = args.first() else {
                    return Err(InterpreterError::TypeError(
                        "methodcaller needs at least one argument, the method name".into(),
                    )
                    .into());
                };
                return Ok(Value::OperatorGetter(Box::new(
                    crate::value::OperatorGetter::MethodCaller {
                        name: name.to_string(),
                        args: args[1..].to_vec(),
                        kwargs: kwargs.clone(),
                    },
                )));
            }
            _ => {}
        }
        if let Some(op) = binary_operator(func) {
            let (a, b) = arg2(func, args)?;
            return crate::eval::op::binop(state, op, a, b, tools).await;
        }
        if let Some(cmp) = compare_op(func) {
            let (a, b) = arg2(func, args)?;
            let (result, _, _) = crate::eval::op::compare(state, cmp, a, b, tools).await?;
            return Ok(Value::Bool(result));
        }
        match func {
            // Sequence concatenation is `+`.
            "concat" => {
                let (a, b) = arg2(func, args)?;
                crate::eval::op::binop(state, Operator::Add, a, b, tools).await
            }
            "neg" => {
                crate::eval::operations::apply_unaryop(
                    state,
                    UnaryOp::USub,
                    arg1(func, args)?,
                    tools,
                )
                .await
            }
            "pos" => {
                crate::eval::operations::apply_unaryop(
                    state,
                    UnaryOp::UAdd,
                    arg1(func, args)?,
                    tools,
                )
                .await
            }
            "invert" => {
                crate::eval::operations::apply_unaryop(
                    state,
                    UnaryOp::Invert,
                    arg1(func, args)?,
                    tools,
                )
                .await
            }
            "not_" => {
                let truthy = crate::eval::op::truthy(state, arg1(func, args)?, tools).await?;
                Ok(Value::Bool(!truthy))
            }
            "truth" => {
                let truthy = crate::eval::op::truthy(state, arg1(func, args)?, tools).await?;
                Ok(Value::Bool(truthy))
            }
            // `operator.contains(a, b)` is `b in a`.
            "contains" => {
                let (a, b) = arg2(func, args)?;
                Ok(Value::Bool(crate::eval::op::contains(state, a, b, tools).await?))
            }
            "getitem" => {
                let (a, b) = arg2(func, args)?;
                crate::eval::op::getitem(state, a, b, tools).await
            }
            "abs" => operator_abs(arg1(func, args)?),
            "index" => operator_index(arg1(func, args)?),
            "is_" => {
                let (a, b) = arg2(func, args)?;
                Ok(Value::Bool(crate::eval::operations::values_is(a, b)))
            }
            "is_not" => {
                let (a, b) = arg2(func, args)?;
                Ok(Value::Bool(!crate::eval::operations::values_is(a, b)))
            }
            // `countOf(a, b)` / `indexOf(a, b)`: occurrences / first index of `b`
            // in the sequence `a`.
            "countOf" => {
                let (a, b) = arg2(func, args)?;
                let items = crate::eval::control_flow::iterate_value(a)?;
                let mut count = 0i64;
                for item in &items {
                    if crate::eval::op::eq(state, item, b, tools).await? {
                        count += 1;
                    }
                }
                Ok(Value::Int(count))
            }
            "indexOf" => {
                let (a, b) = arg2(func, args)?;
                let items = crate::eval::control_flow::iterate_value(a)?;
                for (i, item) in items.iter().enumerate() {
                    if crate::eval::op::eq(state, item, b, tools).await? {
                        return Ok(Value::Int(crate::eval::functions::to_len_i64(i)?));
                    }
                }
                Err(EvalError::Exception(crate::value::ExceptionValue::new(
                    "ValueError",
                    "sequence.index(x): x not in sequence",
                )))
            }
            _ => Err(InterpreterError::AttributeError(format!(
                "module 'operator' has no attribute '{func}'"
            ))
            .into()),
        }
    }
}

/// `operator.abs(x)` — `abs(x)` for the numeric tower (instance `__abs__` is not
/// consulted here; that path is the `abs()` builtin).
fn operator_abs(value: &Value) -> EvalResult {
    use num_traits::Signed as _;
    Ok(match value {
        Value::Int(i) => i.checked_abs().map_or_else(
            || crate::value::int_from_bigint(-num_bigint::BigInt::from(*i)),
            Value::Int,
        ),
        Value::BigInt(b) => crate::value::int_from_bigint((**b).abs()),
        Value::Float(f) => Value::Float(f.abs()),
        Value::Complex(c) => Value::Float(c.norm()),
        Value::Bool(b) => Value::Int(i64::from(*b)),
        Value::Decimal(d, _) => Value::Decimal(Box::new(d.abs()), false),
        Value::Fraction(fr) => Value::Fraction(Box::new((**fr).abs())),
        _ => {
            return Err(InterpreterError::TypeError(format!(
                "bad operand type for abs(): '{}'",
                value.type_name()
            ))
            .into());
        }
    })
}

/// `operator.index(x)` — losslessly convert `x` to an int (`x.__index__()`).
fn operator_index(value: &Value) -> EvalResult {
    match value {
        Value::Int(_) | Value::BigInt(_) => Ok(value.clone()),
        Value::Bool(b) => Ok(Value::Int(i64::from(*b))),
        _ => Err(InterpreterError::TypeError(format!(
            "'{}' object cannot be interpreted as an integer",
            value.type_name()
        ))
        .into()),
    }
}
