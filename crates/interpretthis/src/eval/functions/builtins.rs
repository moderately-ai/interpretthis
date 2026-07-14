// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use indexmap::IndexMap;
use rustpython_parser::ast;

use super::{
    check_arg_count,
    dispatch::call_value_as_function,
    float_to_int_exact,
    helpers::{
        SortRequest, apply_key_fn, bytes_from_int_items, check_isinstance, dsu_sort, object_id,
        parse_int_str, pow_three_arg, type_arg_name,
    },
    method_dispatch::CallArgs,
    resolve_proxy, to_len_i64, value_to_i64,
};
use crate::{
    error::{EvalError, InterpreterError},
    eval::literals::value_to_key,
    state::InterpreterState,
    tools::Tools,
    value::{ExceptionValue, Value, ValueKey, shared_list},
};

/// Check if a name is a known Python exception type.
pub fn is_exception_type_name(name: &str) -> bool {
    matches!(
        name,
        "Exception"
            | "ValueError"
            | "TypeError"
            | "KeyError"
            | "IndexError"
            | "AttributeError"
            | "RuntimeError"
            | "StopIteration"
            | "ZeroDivisionError"
            | "OverflowError"
            | "AssertionError"
            | "NotImplementedError"
            | "FileNotFoundError"
            | "IOError"
            | "OSError"
            | "NameError"
            | "ArithmeticError"
            | "LookupError"
            | "ExceptionGroup"
            | "BaseExceptionGroup"
    )
}

/// Try to dispatch a builtin function. Returns Ok(Some(val)) if handled, Ok(None) if not a builtin.
pub(super) async fn try_builtin(
    state: &mut InterpreterState,
    name: &str,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> Result<Option<Value>, EvalError> {
    match name {
        "print" => {
            let mut resolved_args = Vec::with_capacity(args.len());
            for arg in args {
                resolved_args.push(resolve_proxy(arg).await?);
            }
            let mut parts: Vec<String> = Vec::with_capacity(resolved_args.len());
            for a in &resolved_args {
                parts.push(
                    crate::eval::render::render(
                        state,
                        a,
                        crate::eval::render::RenderMode::Display,
                        tools,
                    )
                    .await?,
                );
            }
            let sep = kwargs.get("sep").map_or_else(|| " ".to_string(), |v| format!("{v}"));
            let end = kwargs.get("end").map_or_else(|| "\n".to_string(), |v| format!("{v}"));
            state.append_print(&parts.join(&sep)).map_err(EvalError::Interpreter)?;
            state.append_print(&end).map_err(EvalError::Interpreter)?;
            Ok(Some(Value::None))
        }
        "len" => {
            check_arg_count(name, args, 1, 1)?;
            let length = crate::eval::op::len(state, &args[0], tools).await?;
            Ok(Some(Value::Int(to_len_i64(length)?)))
        }
        "range" => {
            let (start, stop, stride) = match args.len() {
                1 => (0, value_to_i64(&args[0])?, 1),
                2 => (value_to_i64(&args[0])?, value_to_i64(&args[1])?, 1),
                3 => (value_to_i64(&args[0])?, value_to_i64(&args[1])?, value_to_i64(&args[2])?),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "range expected at most 3 arguments".into(),
                    )
                    .into());
                }
            };
            if stride == 0 {
                return Err(
                    InterpreterError::ValueError("range() arg 3 must not be zero".into()).into()
                );
            }
            Ok(Some(Value::Range { start, stop, step: stride }))
        }
        "str" => {
            check_arg_count(name, args, 0, 1)?;
            if args.is_empty() {
                Ok(Some(Value::String("".into())))
            } else {
                let resolved = resolve_proxy(&args[0]).await?;
                // Route through render() so user-class `__str__`
                // (and the CPython str→repr fallback) dispatches.
                let rendered = crate::eval::render::render(
                    state,
                    &resolved,
                    crate::eval::render::RenderMode::Display,
                    tools,
                )
                .await?;
                Ok(Some(Value::String(rendered.into())))
            }
        }
        "int" => {
            check_arg_count(name, args, 0, 2)?;
            if args.is_empty() {
                return Ok(Some(Value::Int(0)));
            }
            // An explicit base only applies to a string. CPython: `int(255, 16)`
            // raises TypeError, and any base outside {0} ∪ [2, 36] raises
            // ValueError (the old code passed the base straight to
            // `from_str_radix`, which PANICS on 0 or > 36).
            if args.len() >= 2 {
                let base = value_to_i64(&args[1])?;
                return match &args[0] {
                    Value::String(s) => Ok(Some(parse_int_str(s, base)?)),
                    Value::Bytes(b) => {
                        let s = std::str::from_utf8(b).map_err(|_| {
                            EvalError::from(InterpreterError::ValueError(
                                "int() bytes argument is not valid UTF-8".into(),
                            ))
                        })?;
                        Ok(Some(parse_int_str(s, base)?))
                    }
                    _ => Err(InterpreterError::TypeError(
                        "int() can't convert non-string with explicit base".into(),
                    )
                    .into()),
                };
            }
            match &args[0] {
                Value::Int(i) => Ok(Some(Value::Int(*i))),
                // Was missing: `int(2**70)` fell to the catch-all and raised
                // "int() argument must be a string or a number, not 'int'".
                Value::BigInt(b) => Ok(Some(Value::BigInt(b.clone()))),
                Value::Float(f) => Ok(Some(float_to_int_exact(*f)?)),
                Value::Bool(b) => Ok(Some(Value::Int(i64::from(*b)))),
                Value::String(s) => Ok(Some(parse_int_str(s, 10)?)),
                _ => Err(InterpreterError::TypeError(format!(
                    "int() argument must be a string or a number, not '{}'",
                    args[0].type_name()
                ))
                .into()),
            }
        }
        "float" => {
            check_arg_count(name, args, 0, 1)?;
            if args.is_empty() {
                return Ok(Some(Value::Float(0.0)));
            }
            match &args[0] {
                Value::Int(i) => Ok(Some(Value::Float(*i as f64))),
                Value::Float(f) => Ok(Some(Value::Float(*f))),
                Value::Bool(b) => Ok(Some(Value::Float(if *b { 1.0 } else { 0.0 }))),
                Value::String(s) => {
                    let trimmed = s.trim();
                    if trimmed == "inf" || trimmed == "Infinity" {
                        return Ok(Some(Value::Float(f64::INFINITY)));
                    }
                    if trimmed == "-inf" || trimmed == "-Infinity" {
                        return Ok(Some(Value::Float(f64::NEG_INFINITY)));
                    }
                    if trimmed == "nan" {
                        return Ok(Some(Value::Float(f64::NAN)));
                    }
                    let f = trimmed.parse::<f64>().map_err(|_| {
                        EvalError::Exception(ExceptionValue::new(
                            "ValueError",
                            format!("could not convert string to float: '{trimmed}'"),
                        ))
                    })?;
                    Ok(Some(Value::Float(f)))
                }
                _ => Err(InterpreterError::TypeError(format!(
                    "float() argument must be a string or a number, not '{}'",
                    args[0].type_name()
                ))
                .into()),
            }
        }
        "bool" => {
            check_arg_count(name, args, 0, 1)?;
            if args.is_empty() {
                return Ok(Some(Value::Bool(false)));
            }
            let truthy = crate::eval::op::truthy(state, &args[0], tools).await?;
            Ok(Some(Value::Bool(truthy)))
        }
        "type" => {
            // One-arg: type(x). Three-arg: type(name, bases, dict) dynamic class.
            if args.len() == 3 {
                return Ok(Some(crate::eval::classes::dynamic_type_new(
                    state, &args[0], &args[1], &args[2],
                )?));
            }
            check_arg_count(name, args, 1, 1)?;
            // `type(x)` yields a type object: the class object for an instance
            // (so `type(p) is P` and `type(p).__name__ == 'P'`), and a built-in
            // type object otherwise (`type(1).__name__ == 'int'`). A type's own
            // type is `type`.
            let type_obj = match &args[0] {
                Value::Instance(inst) => Value::Class(inst.class_name.clone()),
                // Exception variant carries its concrete type_name on
                // the value itself (ValueError, KeyError, …). Without
                // this arm `type(e).__name__` collapses every variant
                // to the static `"Exception"` label.
                Value::Exception(exc) => Value::ExceptionType(exc.type_name.clone()),
                Value::Type(_) | Value::Class(_) => Value::Type("type".to_string()),
                Value::Module(_) => Value::Type("module".to_string()),
                other => Value::Type(other.type_name().to_string()),
            };
            Ok(Some(type_obj))
        }
        "isinstance" => {
            check_arg_count(name, args, 2, 2)?;
            let obj = &args[0];
            // isinstance(x, (A, B)) matches if any element matches.
            if let Value::Tuple(items) = &args[1] {
                let any =
                    items.iter().any(|item| check_isinstance(state, obj, &type_arg_name(item)));
                return Ok(Some(Value::Bool(any)));
            }
            Ok(Some(Value::Bool(check_isinstance(state, obj, &type_arg_name(&args[1])))))
        }
        "super" => {
            // Two forms supported (Track B1):
            //  * super() — zero-arg, reads the current method frame (defining class + self) from
            //    state.method_frame_stack.
            //  * super(Cls, self) — explicit two-arg form.
            // CPython's one-arg form (`super(Cls)` returning an unbound
            // proxy) is not commonly used and not modelled.
            match args.len() {
                0 => {
                    let Some(frame) = state.method_frame_stack.last() else {
                        return Err(InterpreterError::Runtime(
                            "super(): no current method frame (super() must be called inside a method)".into(),
                        )
                        .into());
                    };
                    // Re-read self from the local variable when
                    // possible. This is load-bearing for sequential
                    // `super().__setattr__(...)` calls: after the
                    // first call updates the local `self`, the
                    // second super() needs to see the updated
                    // instance, not the snapshot frame.self_value.
                    let defining_class = frame.defining_class.clone();
                    let self_local = frame.self_local_name.clone();
                    let live_self = self_local
                        .as_deref()
                        .and_then(|n| state.get_variable(n).cloned())
                        .unwrap_or_else(|| frame.self_value.clone());
                    let Value::Instance(inst) = &live_self else {
                        return Err(InterpreterError::Runtime(
                            "super(): current self is not an instance".into(),
                        )
                        .into());
                    };
                    Ok(Some(Value::Super { defining_class, instance: Box::new(inst.clone()) }))
                }
                2 => {
                    let Value::Class(cls_name) = &args[0] else {
                        return Err(InterpreterError::TypeError(
                            "super() argument 1 must be a class".into(),
                        )
                        .into());
                    };
                    let Value::Instance(inst) = &args[1] else {
                        return Err(InterpreterError::TypeError(
                            "super() argument 2 must be an instance".into(),
                        )
                        .into());
                    };
                    // Validate the relationship — CPython raises TypeError
                    // when argument 2's class doesn't have argument 1 in
                    // its MRO. This catches `super(Unrelated, self)`
                    // misuse at the construction site.
                    let in_mro = state
                        .classes
                        .get(&inst.class_name)
                        .is_some_and(|c| c.mro.iter().any(|a| a == cls_name));
                    if !in_mro {
                        return Err(InterpreterError::TypeError(format!(
                            "super(type, obj): obj must be an instance or subtype of type '{cls_name}'"
                        ))
                        .into());
                    }
                    Ok(Some(Value::Super {
                        defining_class: cls_name.clone(),
                        instance: Box::new(inst.clone()),
                    }))
                }
                _ => {
                    Err(InterpreterError::TypeError("super() takes 0 or 2 arguments".into()).into())
                }
            }
        }
        "issubclass" => {
            check_arg_count(name, args, 2, 2)?;
            // issubclass(C, B): True iff B is in C's MRO. C must be a
            // class value; B can be a single class or a tuple of
            // classes. CPython raises TypeError when arg1 isn't a class.
            let Value::Class(child_name) = &args[0] else {
                return Err(InterpreterError::TypeError(
                    "issubclass() arg 1 must be a class".into(),
                )
                .into());
            };
            let class = state.classes.get(child_name);
            let check_one = |target_name: &str| -> bool {
                if target_name == "object" {
                    return true;
                }
                class.is_some_and(|c| c.mro.iter().any(|a| a == target_name))
            };
            if let Value::Tuple(items) = &args[1] {
                let any = items.iter().any(|item| check_one(&type_arg_name(item)));
                return Ok(Some(Value::Bool(any)));
            }
            Ok(Some(Value::Bool(check_one(&type_arg_name(&args[1])))))
        }
        "getattr" => {
            // Bounded: attribute name must be a string and pass
            // BLOCKED_ATTRIBUTES. Two-arg raises AttributeError on miss;
            // three-arg returns the default instead.
            check_arg_count(name, args, 2, 3)?;
            let attr_name = match &args[1] {
                Value::String(s) => s.as_str(),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "getattr(): attribute name must be string".into(),
                    )
                    .into());
                }
            };
            let obj = resolve_proxy(&args[0]).await?;
            match crate::eval::names::getattr_on_value(state, obj, attr_name, tools, None).await {
                Ok(v) => Ok(Some(v)),
                // Default only swallows AttributeError — Security on blocked
                // dunders stays a hard failure.
                Err(EvalError::Interpreter(InterpreterError::AttributeError(_)))
                    if args.len() >= 3 =>
                {
                    Ok(Some(args[2].clone()))
                }
                Err(e) => Err(e),
            }
        }
        "setattr" => {
            check_arg_count(name, args, 3, 3)?;
            let attr_name = match &args[1] {
                Value::String(s) => s.as_str(),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "setattr(): attribute name must be string".into(),
                    )
                    .into());
                }
            };
            crate::security::validator::validate_attribute(attr_name)?;
            let obj = resolve_proxy(&args[0]).await?;
            let value = resolve_proxy(&args[2]).await?;
            match obj {
                Value::Instance(inst) => {
                    if state.classes.get(&inst.class_name).is_some_and(|c| c.frozen) {
                        return Err(EvalError::Exception(ExceptionValue::new(
                            "FrozenInstanceError",
                            format!("cannot assign to field '{attr_name}'"),
                        )));
                    }
                    // Shared fields: mutation is visible on every alias.
                    inst.fields.lock().insert(attr_name.to_string(), value);
                    Ok(Some(Value::None))
                }
                other => Err(InterpreterError::TypeError(format!(
                    "setattr() attribute assignment not supported for '{}'",
                    other.type_name()
                ))
                .into()),
            }
        }
        "delattr" => {
            check_arg_count(name, args, 2, 2)?;
            let attr_name = match &args[1] {
                Value::String(s) => s.as_str(),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "delattr(): attribute name must be string".into(),
                    )
                    .into());
                }
            };
            crate::security::validator::validate_attribute(attr_name)?;
            let obj = resolve_proxy(&args[0]).await?;
            match obj {
                Value::Instance(inst) => {
                    if inst.fields.lock().remove(attr_name).is_none() {
                        return Err(
                            InterpreterError::AttributeError(format!("'{attr_name}'")).into()
                        );
                    }
                    Ok(Some(Value::None))
                }
                other => Err(InterpreterError::TypeError(format!(
                    "delattr() attribute deletion not supported for '{}'",
                    other.type_name()
                ))
                .into()),
            }
        }
        "hasattr" => {
            check_arg_count(name, args, 2, 2)?;
            let attr_name = match &args[1] {
                Value::String(s) => s.as_str(),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "hasattr(): attribute name must be string".into(),
                    )
                    .into());
                }
            };
            // Reject blocked dunders as missing (hasattr returns False on
            // AttributeError; Security on blocked names → False for parity
            // with "cannot access" without leaking existence).
            if crate::security::validator::validate_attribute(attr_name).is_err() {
                return Ok(Some(Value::Bool(false)));
            }
            // CPython hasattr(obj, name): True iff getattr(obj, name)
            // doesn't raise. Route through dispatch_getattr_opt first
            // (covers every builtin-with-attributes type); if the slot
            // is None for this variant, consult Instance / Class /
            // Exception fallbacks directly so hasattr stays in sync
            // with eval_attribute's state-aware path.
            let has = match crate::types::dispatch_getattr_opt(&args[0], attr_name) {
                Ok(Some(_)) => true,
                Ok(None) => match &args[0] {
                    Value::Instance(inst) => {
                        inst.fields.lock().contains_key(attr_name)
                            || state.classes.get(&inst.class_name).is_some_and(|c| {
                                c.class_attrs.contains_key(attr_name)
                                    || c.methods.contains_key(attr_name)
                            })
                    }
                    Value::Class(class_name) => {
                        attr_name == "__name__"
                            || attr_name == "__qualname__"
                            || state.classes.get(class_name).is_some_and(|c| {
                                c.class_attrs.contains_key(attr_name)
                                    || c.methods.contains_key(attr_name)
                            })
                    }
                    Value::Exception(_) => attr_name == "message" || attr_name == "args",
                    Value::Type(_) | Value::Function(_) | Value::Lambda(_) => {
                        attr_name == "__name__" || attr_name == "__qualname__"
                    }
                    Value::Module(module) => {
                        crate::eval::modules::module_member(module, attr_name).is_ok()
                    }
                    Value::Date(_) => {
                        matches!(attr_name, "year" | "month" | "day" | "isoformat" | "weekday")
                    }
                    _ => false,
                },
                Err(_) => false,
            };
            Ok(Some(Value::Bool(has)))
        }
        "callable" => {
            check_arg_count(name, args, 1, 1)?;
            // Every value shape that `call_value_as_function` accepts.
            // Class objects are callable (instantiation). The typed
            // bare-name sentinels (BuiltinName, ToolName, ExceptionType,
            // UnboundClassMethod) are all callable. Method markers
            // (BoundMethod / BuiltinTypeMethod) and ModuleFunction are
            // callable. Anything else is NOT callable (CPython parity).
            let is_callable = matches!(
                &args[0],
                Value::Function(_)
                    | Value::Lambda(_)
                    | Value::Class(_)
                    | Value::BoundMethod { .. }
                    | Value::BuiltinTypeMethod { .. }
                    | Value::ModuleFunction { .. }
                    | Value::BuiltinName(_)
                    | Value::ToolName(_)
                    | Value::ExceptionType(_)
                    | Value::ExceptionMethod { .. }
                    | Value::UnboundClassMethod { .. }
                    | Value::Partial(_)
                    | Value::LruCache(_)
            );
            Ok(Some(Value::Bool(is_callable)))
        }
        "abs" => {
            check_arg_count(name, args, 1, 1)?;
            match &args[0] {
                // `checked_abs` handles i64::MIN, whose abs overflows i64:
                // promote to BigInt instead of panicking (debug) / wrapping.
                Value::Int(i) => Ok(Some(i.checked_abs().map_or_else(
                    || crate::value::int_from_bigint(-num_bigint::BigInt::from(*i)),
                    Value::Int,
                ))),
                Value::BigInt(b) => {
                    use num_traits::Signed as _;
                    Ok(Some(crate::value::int_from_bigint((**b).abs())))
                }
                Value::Float(f) => Ok(Some(Value::Float(f.abs()))),
                Value::Bool(b) => Ok(Some(Value::Int(i64::from(*b)))),
                Value::Decimal(d) => Ok(Some(Value::Decimal(Box::new(d.abs())))),
                Value::Fraction(fr) => {
                    use num_traits::Signed as _;
                    Ok(Some(Value::Fraction(Box::new((**fr).abs()))))
                }
                _ => Err(InterpreterError::TypeError(format!(
                    "bad operand type for abs(): '{}'",
                    args[0].type_name()
                ))
                .into()),
            }
        }
        "round" => {
            check_arg_count(name, args, 1, 2)?;
            let ndigits = if args.len() >= 2 { Some(value_to_i64(&args[1])?) } else { None };
            match &args[0] {
                // CPython's `round(int, n)` returns an int rounded to the
                // nearest multiple of 10**(-n) for n<0; rounding is
                // banker's. `round(int)` and `round(int, n>=0)` are no-ops.
                Value::Int(i) => match ndigits {
                    None => Ok(Some(Value::Int(*i))),
                    Some(n) if n >= 0 => Ok(Some(Value::Int(*i))),
                    Some(n) => {
                        let abs_exp = u32::try_from(-n).unwrap_or(u32::MAX);
                        // |n| beyond ~19 wipes any i64 out to zero; clamp
                        // at 18 (max safe i64 exponent) and short-circuit
                        // larger to zero — CPython returns 0 too.
                        if abs_exp > 18 {
                            return Ok(Some(Value::Int(0)));
                        }
                        let factor = 10_i64.pow(abs_exp);
                        // Banker's round: truncated divide, then if the
                        // remainder is exactly half the divisor pick the
                        // even quotient. Handle negatives symmetrically
                        // (Rust's `/` truncates toward zero, so for negative
                        // `i` we step the quotient further away from zero
                        // on a round-up instead of toward).
                        let q = i / factor;
                        let r = i - q * factor;
                        let twice_r = r.abs() * 2;
                        let rounded = match twice_r.cmp(&factor) {
                            std::cmp::Ordering::Equal => {
                                if q % 2 == 0 {
                                    q
                                } else if i.is_negative() {
                                    q - 1
                                } else {
                                    q + 1
                                }
                            }
                            std::cmp::Ordering::Greater => {
                                if i.is_negative() {
                                    q - 1
                                } else {
                                    q + 1
                                }
                            }
                            std::cmp::Ordering::Less => q,
                        };
                        Ok(Some(Value::Int(rounded * factor)))
                    }
                },
                // CPython's `round()` uses IEEE-754 round-half-to-even
                // (banker's rounding): `round(0.5) == 0`, `round(2.5) == 2`,
                // `round(-0.5) == 0`. Rust's `f64::round()` is
                // round-half-away-from-zero — wrong for parity. Use
                // `round_ties_even()` which implements the IEEE rule.
                Value::Float(f) => ndigits.map_or_else(
                    || Ok(Some(float_to_int_exact(f.round_ties_even())?)),
                    |n| {
                        // CPython's `round(x, n)` uses correctly-rounded
                        // decimal formatting (via dtoa), not naive
                        // multiply-round-divide. The multiply-divide
                        // approach breaks because some decimals like
                        // 2.675 multiply to exactly 267.5 in IEEE-754,
                        // which then rounds up to 268 → 2.68; CPython
                        // gets 2.67 because dtoa sees the underlying
                        // 2.6749999... and emits "2.67". Rust's `{:.n$}`
                        // formatter uses the same correctly-rounded
                        // decimal algorithm (Ryu/Grisu3) and parses back
                        // to give the same answer.
                        //
                        // CPython also honors negative ndigits (round to
                        // the nearest 10^|n|), which the format-and-parse
                        // path can't express. Fall back to the
                        // multiply/divide form when n < 0 — banker's
                        // rounding via `round_ties_even` on the scaled
                        // value matches CPython for the common cases
                        // (`round(125.0, -1) == 120.0`).
                        if n >= 0 {
                            let places = usize::try_from(n).unwrap_or(usize::MAX);
                            let s = format!("{f:.places$}");
                            let parsed = s.parse::<f64>().unwrap_or(*f);
                            return Ok(Some(Value::Float(parsed)));
                        }
                        let exp =
                            i32::try_from(n).unwrap_or(if n > 0 { i32::MAX } else { i32::MIN });
                        let factor = 10f64.powi(exp);
                        Ok(Some(Value::Float((f * factor).round_ties_even() / factor)))
                    },
                ),
                Value::Bool(b) => Ok(Some(Value::Int(i64::from(*b)))),
                _ => Err(InterpreterError::TypeError(format!(
                    "type '{}' doesn't define __round__",
                    args[0].type_name()
                ))
                .into()),
            }
        }
        "min" => {
            if args.is_empty() {
                return Err(
                    InterpreterError::TypeError("min expected at least 1 argument".into()).into()
                );
            }
            let items = if args.len() == 1 {
                crate::eval::op::iter(state, &args[0], tools).await?
            } else {
                args.to_vec()
            };
            if items.is_empty() {
                // `default` keyword: returned when the iterable is
                // empty instead of raising ValueError.
                if let Some(default) = kwargs.get("default") {
                    return Ok(Some(default.clone()));
                }
                return Err(
                    InterpreterError::ValueError("min() arg is an empty sequence".into()).into()
                );
            }
            let key_fn = kwargs.get("key");
            let mut min_val = items[0].clone();
            let mut min_key = apply_key_fn(state, &min_val, key_fn, tools).await?;
            for item in items.iter().skip(1) {
                let item_key = apply_key_fn(state, item, key_fn, tools).await?;
                if crate::eval::operations::compare_lt(&item_key, &min_key)? {
                    min_val = item.clone();
                    min_key = item_key;
                }
            }
            Ok(Some(min_val))
        }
        "max" => {
            if args.is_empty() {
                return Err(
                    InterpreterError::TypeError("max expected at least 1 argument".into()).into()
                );
            }
            let items = if args.len() == 1 {
                crate::eval::op::iter(state, &args[0], tools).await?
            } else {
                args.to_vec()
            };
            if items.is_empty() {
                // `default` keyword: returned when the iterable is
                // empty instead of raising ValueError.
                if let Some(default) = kwargs.get("default") {
                    return Ok(Some(default.clone()));
                }
                return Err(
                    InterpreterError::ValueError("max() arg is an empty sequence".into()).into()
                );
            }
            let key_fn = kwargs.get("key");
            let mut max_val = items[0].clone();
            let mut max_key = apply_key_fn(state, &max_val, key_fn, tools).await?;
            for item in items.iter().skip(1) {
                let item_key = apply_key_fn(state, item, key_fn, tools).await?;
                if crate::eval::operations::compare_lt(&max_key, &item_key)? {
                    max_val = item.clone();
                    max_key = item_key;
                }
            }
            Ok(Some(max_val))
        }
        "sum" => {
            check_arg_count(name, args, 1, 2)?;
            let items = crate::eval::op::iter(state, &args[0], tools).await?;
            let start = if args.len() >= 2 { args[1].clone() } else { Value::Int(0) };
            let mut total = start;
            for item in items {
                total = crate::eval::operations::apply_binop(
                    &total,
                    &item,
                    ast::Operator::Add,
                    state.decimal_prec,
                    state.config.max_int_bits,
                )?;
            }
            Ok(Some(total))
        }
        "all" => {
            check_arg_count(name, args, 1, 1)?;
            let items = crate::eval::op::iter(state, &args[0], tools).await?;
            Ok(Some(Value::Bool(items.iter().all(Value::is_truthy))))
        }
        "any" => {
            check_arg_count(name, args, 1, 1)?;
            let items = crate::eval::op::iter(state, &args[0], tools).await?;
            Ok(Some(Value::Bool(items.iter().any(Value::is_truthy))))
        }
        "sorted" => {
            // `sorted` takes exactly one positional (the iterable); `key` and
            // `reverse` are keyword-only. The old code checked only for zero
            // args, so `sorted(xs, keyfn)` silently ignored the second argument.
            if args.len() != 1 {
                return Err(InterpreterError::TypeError(format!(
                    "sorted expected 1 argument, got {}",
                    args.len()
                ))
                .into());
            }
            let req = SortRequest {
                items: crate::eval::op::iter(state, &args[0], tools).await?,
                key_fn: kwargs.get("key"),
                reverse: kwargs.get("reverse").is_some_and(Value::is_truthy),
            };
            let sorted = dsu_sort(state, tools, req).await?;
            Ok(Some(Value::List(shared_list(sorted))))
        }
        "enumerate" => {
            check_arg_count(name, args, 1, 2)?;
            let items = crate::eval::op::iter(state, &args[0], tools).await?;
            // `start` accepted positionally OR via keyword.
            let start = if args.len() >= 2 {
                value_to_i64(&args[1])?
            } else if let Some(s) = kwargs.get("start") {
                value_to_i64(s)?
            } else {
                0
            };
            let mut result = Vec::with_capacity(items.len());
            for (i, v) in items.into_iter().enumerate() {
                result.push(Value::Tuple(vec![Value::Int(start + to_len_i64(i)?), v]));
            }
            Ok(Some(Value::List(shared_list(result))))
        }
        "zip" => {
            if args.is_empty() {
                return Ok(Some(Value::List(shared_list(Vec::new()))));
            }
            let strict = kwargs.get("strict").is_some_and(Value::is_truthy);
            let mut iterables: Vec<Vec<Value>> = Vec::with_capacity(args.len());
            for arg in args {
                iterables.push(crate::eval::op::iter(state, arg, tools).await?);
            }
            // `strict=True` raises ValueError when lengths differ.
            // CPython's message names the FIRST mismatched argument.
            if strict {
                if let Some(first_len) = iterables.first().map(Vec::len) {
                    for (i, it) in iterables.iter().enumerate().skip(1) {
                        if it.len() != first_len {
                            let shorter = it.len() < first_len;
                            let arg_num = if shorter { i + 1 } else { 1 };
                            let direction = if shorter { "shorter" } else { "longer" };
                            return Err(InterpreterError::ValueError(format!(
                                "zip() argument {arg_num} is {direction} than argument 1"
                            ))
                            .into());
                        }
                    }
                }
            }
            let min_len = iterables.iter().map(std::vec::Vec::len).min().unwrap_or(0);
            let mut result = Vec::new();
            for i in 0..min_len {
                let tuple: Vec<Value> = iterables.iter().map(|it| it[i].clone()).collect();
                result.push(Value::Tuple(tuple));
            }
            Ok(Some(Value::List(shared_list(result))))
        }
        "reversed" => {
            check_arg_count(name, args, 1, 1)?;
            let mut items = crate::eval::op::iter(state, &args[0], tools).await?;
            items.reverse();
            Ok(Some(Value::List(shared_list(items))))
        }
        "chr" => {
            check_arg_count(name, args, 1, 1)?;
            let code = value_to_i64(&args[0])?;
            // Python's chr() accepts 0..=0x10FFFF; out-of-range ints yield
            // ValueError. Convert to u32 defensively — a negative or >u32
            // code is caught by the same ValueError path via `from_u32`.
            let code_u32 = u32::try_from(code).map_err(|_| {
                EvalError::Exception(ExceptionValue::new(
                    "ValueError",
                    "chr() arg not in range(0x110000)",
                ))
            })?;
            let ch = char::from_u32(code_u32).ok_or_else(|| {
                EvalError::Exception(ExceptionValue::new(
                    "ValueError",
                    "chr() arg not in range(0x110000)",
                ))
            })?;
            Ok(Some(Value::String(ch.to_string().into())))
        }
        "ord" => {
            check_arg_count(name, args, 1, 1)?;
            if let Value::String(s) = &args[0] {
                let chars: Vec<char> = s.chars().collect();
                if chars.len() != 1 {
                    return Err(EvalError::Exception(ExceptionValue::new(
                        "TypeError",
                        format!(
                            "ord() expected a character, but string of length {} found",
                            chars.len()
                        ),
                    )));
                }
                // char is at most 21 bits (U+10FFFF), so u32 -> i64 via
                // i64::from is lossless and cleaner than `as`.
                Ok(Some(Value::Int(i64::from(u32::from(chars[0])))))
            } else {
                Err(InterpreterError::TypeError(format!(
                    "ord() expected string of length 1, but {} found",
                    args[0].type_name()
                ))
                .into())
            }
        }
        "list" => {
            check_arg_count(name, args, 0, 1)?;
            if args.is_empty() {
                return Ok(Some(Value::List(shared_list(Vec::new()))));
            }
            let items = crate::eval::op::iter(state, &args[0], tools).await?;
            Ok(Some(Value::List(shared_list(items))))
        }
        "tuple" => {
            check_arg_count(name, args, 0, 1)?;
            if args.is_empty() {
                return Ok(Some(Value::Tuple(Vec::new())));
            }
            let items = crate::eval::op::iter(state, &args[0], tools).await?;
            Ok(Some(Value::Tuple(items)))
        }
        "dict" => {
            check_arg_count(name, args, 0, 1)?;
            if args.is_empty() && kwargs.is_empty() {
                return Ok(Some(Value::Dict(IndexMap::new())));
            }
            let mut map = IndexMap::new();
            if !args.is_empty() {
                // dict from iterable of pairs
                let items = crate::eval::op::iter(state, &args[0], tools).await?;
                for item in items {
                    if let Value::Tuple(pair) = &item {
                        if pair.len() == 2 {
                            let key = value_to_key(&pair[0])?;
                            map.insert(key, pair[1].clone());
                        } else {
                            return Err(InterpreterError::TypeError(
                                "dict() requires key-value pairs".into(),
                            )
                            .into());
                        }
                    } else if let Value::List(pair) = &item {
                        // Snapshot the pair so the lock guard's scope
                        // doesn't span the value_to_key error path.
                        let snapshot = pair.lock().clone();
                        if snapshot.len() == 2 {
                            let key = value_to_key(&snapshot[0])?;
                            map.insert(key, snapshot[1].clone());
                        } else {
                            return Err(InterpreterError::TypeError(
                                "dict() requires key-value pairs".into(),
                            )
                            .into());
                        }
                    } else {
                        return Err(InterpreterError::TypeError(
                            "dict() requires an iterable of pairs".into(),
                        )
                        .into());
                    }
                }
            }
            for (k, v) in kwargs {
                map.insert(ValueKey::String(k.clone().into()), v.clone());
            }
            Ok(Some(Value::Dict(map)))
        }
        "set" => {
            check_arg_count(name, args, 0, 1)?;
            if args.is_empty() {
                return Ok(Some(Value::Set(Vec::new())));
            }
            let items = crate::eval::op::iter(state, &args[0], tools).await?;
            // Shared set construction: raises on an unhashable element and
            // dedups instances by __eq__ (both of which the old open-coded
            // `value_to_key(x).ok()` dedup got wrong — it silently included
            // unhashables and collapsed every instance to one).
            Ok(Some(crate::eval::literals::build_set(state, items, tools).await?))
        }
        "iter" => {
            check_arg_count(name, args, 1, 2)?;
            // Two-arg form: `iter(callable, sentinel)` calls `callable` with
            // no arguments until the result equals `sentinel`. Eagerly
            // materialise into a list so it slots into the rest of the
            // interpreter's snapshot-iteration model.
            if args.len() == 2 {
                let callable = args[0].clone();
                let sentinel = args[1].clone();
                let mut out: Vec<Value> = Vec::new();
                // Hard upper bound matches `for` loop semantics elsewhere
                // in the interpreter — protects against an infinite
                // callable; CPython has no cap but a malicious snippet
                // can lock the interpreter indefinitely.
                const ITER_CALLABLE_LIMIT: usize = 1_000_000;
                for _ in 0..ITER_CALLABLE_LIMIT {
                    let next =
                        call_value_as_function(state, &callable, &[], &IndexMap::new(), tools)
                            .await?;
                    if next == sentinel {
                        return Ok(Some(Value::List(shared_list(out))));
                    }
                    out.push(next);
                }
                return Err(InterpreterError::Runtime(format!(
                    "iter(callable, sentinel) did not terminate within {ITER_CALLABLE_LIMIT} iterations"
                ))
                .into());
            }
            // One-arg form: return a real iterator that `next()` can advance.
            match &args[0] {
                // Already an iterator — CPython returns it unchanged (iter(it) is it).
                Value::Lazy { .. } | Value::Generator { .. } => Ok(Some(args[0].clone())),
                // A user object that already defines __next__ is its own iterator.
                Value::Instance(inst)
                    if crate::eval::classes::lookup_method_in_mro(
                        state,
                        &inst.class_name,
                        "__next__",
                    )
                    .is_some() =>
                {
                    Ok(Some(args[0].clone()))
                }
                // Any other iterable: materialise into a fresh cursor-backed
                // iterator (a list/tuple/str/range/dict/set is iterable but not
                // itself an iterator). `op::iter` raises TypeError for a
                // non-iterable.
                other => {
                    let items = crate::eval::op::iter(state, other, tools).await?;
                    let cursor_id = state.next_cursor_id;
                    state.next_cursor_id = state.next_cursor_id.wrapping_add(1);
                    state.lazy_cursors.insert(cursor_id, 0);
                    Ok(Some(Value::Lazy { items, cursor_id }))
                }
            }
        }
        "bytes" | "bytearray" => {
            // CPython: bytes() -> b''; bytes(int) -> b'\x00' * n;
            // bytes(iterable_of_ints) -> bytes from each int;
            // bytes(str, encoding) -> str.encode(encoding).
            // We don't distinguish bytes vs bytearray (no mutable
            // variant); both return Value::Bytes.
            if args.is_empty() {
                return Ok(Some(Value::Bytes(Vec::new())));
            }
            match &args[0] {
                Value::Int(n) => {
                    // `bytes(-5)` raises ValueError; the old `.max(0)` silently
                    // produced an empty bytes.
                    let count = usize::try_from(*n).map_err(|_| {
                        EvalError::from(InterpreterError::ValueError("negative count".into()))
                    })?;
                    Ok(Some(Value::Bytes(vec![0u8; count])))
                }
                Value::Bytes(b) => Ok(Some(Value::Bytes(b.clone()))),
                Value::String(s) => {
                    // CPython: bytes(str) without encoding raises
                    // TypeError. With encoding, encodes.
                    let encoding = match args.get(1) {
                        Some(Value::String(e)) => e.as_str(),
                        Some(_) => {
                            return Err(InterpreterError::TypeError(
                                "encoding must be a str".into(),
                            )
                            .into());
                        }
                        None => {
                            return Err(InterpreterError::TypeError(
                                "string argument without an encoding".into(),
                            )
                            .into());
                        }
                    };
                    match encoding {
                        "utf-8" | "utf_8" | "ascii" => {
                            Ok(Some(Value::Bytes(s.as_bytes().to_vec())))
                        }
                        other => {
                            Err(InterpreterError::ValueError(format!("unknown encoding: {other}"))
                                .into())
                        }
                    }
                }
                // Any iterable of ints (list, tuple, range, set, generator, ...).
                // `op::iter` materialises it, raising TypeError for a
                // non-iterable — which is the right error for `bytes(3.5)` too.
                other => {
                    let items = crate::eval::op::iter(state, other, tools).await?;
                    Ok(Some(Value::Bytes(bytes_from_int_items(&items)?)))
                }
            }
        }
        "next" => {
            check_arg_count(name, args, 1, 2)?;
            // Generator iterators: read the cursor, advance, return
            // the item at the old cursor; StopIteration when exhausted
            // (default arg returns it instead, matching CPython's
            // `next(g, sentinel)` shape).
            if let Value::Lazy { items, cursor_id } = &args[0] {
                let cursor = state.lazy_cursors.get(cursor_id).copied().unwrap_or(0);
                if cursor < items.len() {
                    state.lazy_cursors.insert(*cursor_id, cursor + 1);
                    return Ok(Some(items[cursor].clone()));
                }
                return if args.len() >= 2 {
                    Ok(Some(args[1].clone()))
                } else {
                    Err(EvalError::Exception(ExceptionValue::new("StopIteration", String::new())))
                };
            }
            if let Value::Generator { id } = &args[0] {
                let id = *id;
                match super::generators::dispatch_generator_method(
                    state,
                    &Value::Generator { id },
                    "__next__",
                    &[],
                    &IndexMap::new(),
                    tools,
                )
                .await
                {
                    Ok(v) => return Ok(Some(v)),
                    Err(EvalError::Exception(exc)) if exc.type_name == "StopIteration" => {
                        return if args.len() >= 2 {
                            Ok(Some(args[1].clone()))
                        } else {
                            Err(EvalError::Exception(exc))
                        };
                    }
                    Err(e) => return Err(e),
                }
            }
            // For user-class iterators, call __next__ directly so we
            // get exactly one item without materialising the whole
            // sequence. For builtin iterables, materialise and return
            // the first element to match the legacy behaviour.
            if let Value::Instance(inst) = &args[0] {
                if let Some((_, method)) =
                    crate::eval::classes::lookup_method_in_mro(state, &inst.class_name, "__next__")
                {
                    let call = CallArgs { positional: &[], keyword: &IndexMap::new() };
                    let next_result = crate::eval::classes::call_method(
                        state,
                        &method,
                        args[0].clone(),
                        call,
                        tools,
                    )
                    .await;
                    return match next_result {
                        Ok((item, _self)) => Ok(Some(item)),
                        Err(EvalError::Exception(exc))
                            if exc.type_name == "StopIteration" && args.len() >= 2 =>
                        {
                            Ok(Some(args[1].clone()))
                        }
                        Err(other) => Err(other),
                    };
                }
            }
            // Everything else (list, tuple, str, dict, int, ...) is iterable
            // but NOT an iterator, so CPython raises — `next([1,2,3])` is a
            // TypeError, not the first element. The old fallback materialised
            // the iterable and returned items[0] every call (never advancing),
            // and swallowed the not-an-iterator error into the default.
            Err(InterpreterError::TypeError(format!(
                "'{}' object is not an iterator",
                args[0].type_name()
            ))
            .into())
        }
        "filter" => {
            check_arg_count(name, args, 2, 2)?;
            let func = &args[0];
            let iterable = crate::eval::op::iter(state, &args[1], tools).await?;
            let mut result = Vec::new();
            for item in iterable {
                let keep = if matches!(func, Value::None) {
                    item.is_truthy()
                } else {
                    let val = call_value_as_function(
                        state,
                        func,
                        std::slice::from_ref(&item),
                        &IndexMap::new(),
                        tools,
                    )
                    .await?;
                    val.is_truthy()
                };
                if keep {
                    result.push(item);
                }
            }
            Ok(Some(Value::List(shared_list(result))))
        }
        "map" => {
            if args.len() < 2 {
                return Err(InterpreterError::TypeError(
                    "map() requires at least 2 arguments".into(),
                )
                .into());
            }
            let func = &args[0];
            if args.len() == 2 {
                let iterable = crate::eval::op::iter(state, &args[1], tools).await?;
                let mut result = Vec::new();
                for item in iterable {
                    let val = call_value_as_function(state, func, &[item], &IndexMap::new(), tools)
                        .await?;
                    result.push(val);
                }
                Ok(Some(Value::List(shared_list(result))))
            } else {
                // Multiple iterables — zip them
                let mut iterables: Vec<Vec<Value>> = Vec::with_capacity(args.len() - 1);
                for arg in &args[1..] {
                    iterables.push(crate::eval::op::iter(state, arg, tools).await?);
                }
                let min_len = iterables.iter().map(std::vec::Vec::len).min().unwrap_or(0);
                let mut result = Vec::new();
                for i in 0..min_len {
                    let call_args: Vec<Value> = iterables.iter().map(|it| it[i].clone()).collect();
                    let val =
                        call_value_as_function(state, func, &call_args, &IndexMap::new(), tools)
                            .await?;
                    result.push(val);
                }
                Ok(Some(Value::List(shared_list(result))))
            }
        }
        "repr" => {
            check_arg_count(name, args, 1, 1)?;
            // State-aware: `@dataclass`-synthesized __repr__ and
            // user-defined `__repr__` both need the class registry
            // and a method-call channel; render() owns the dispatch.
            let rendered = crate::eval::render::render(
                state,
                &args[0],
                crate::eval::render::RenderMode::Repr,
                tools,
            )
            .await?;
            Ok(Some(Value::String(rendered.into())))
        }
        "hash" => {
            check_arg_count(name, args, 1, 1)?;
            // Route through `op::hash` so user-class `__hash__` runs; the
            // sync `dispatch_hash` path only covers builtins + identity.
            let h = crate::eval::op::hash(state, &args[0], tools).await?;
            Ok(Some(Value::Int(h)))
        }
        "pow" => {
            // CPython: pow(base, exp[, mod]). The 3-arg form is integer
            // modular exponentiation, used heavily in cryptographic code and
            // primality tests. Track A3's parity-wins surface.
            check_arg_count(name, args, 2, 3)?;
            let base = &args[0];
            let exp = &args[1];
            args.get(2).map_or_else(
                || {
                    crate::types::dispatch_binop(
                        crate::types::BinOp::Pow,
                        base,
                        exp,
                        state.decimal_prec,
                    )
                    .map(Some)
                },
                |modulus| pow_three_arg(base, exp, modulus).map(Some),
            )
        }
        "format" => {
            // CPython: `format(value, spec="")`. Routes through
            // `value.__format__(spec)` for user-class instances; for
            // builtins, an empty spec is equivalent to `str(value)`
            // and a non-empty spec applies the format-spec mini-
            // language via `apply_format_spec`.
            check_arg_count(name, args, 1, 2)?;
            let spec = match args.get(1) {
                Some(Value::String(s)) => s.clone(),
                Some(other) => format!("{other}").into(),
                None => "".into(),
            };
            if let Some(rendered) =
                crate::eval::strings::call_format_slot(state, &args[0], &spec, tools).await?
            {
                return Ok(Some(Value::String(rendered.into())));
            }
            if spec.is_empty() {
                let rendered = crate::eval::render::render(
                    state,
                    &args[0],
                    crate::eval::render::RenderMode::Display,
                    tools,
                )
                .await?;
                Ok(Some(Value::String(rendered.into())))
            } else {
                Ok(Some(crate::eval::strings::apply_format_spec(&args[0], &spec)?))
            }
        }
        "bin" => {
            check_arg_count(name, args, 1, 1)?;
            let n = value_to_i64(&args[0])?;
            let formatted =
                if n < 0 { format!("-0b{:b}", n.unsigned_abs()) } else { format!("0b{n:b}") };
            Ok(Some(Value::String(formatted.into())))
        }
        "oct" => {
            check_arg_count(name, args, 1, 1)?;
            let n = value_to_i64(&args[0])?;
            let formatted =
                if n < 0 { format!("-0o{:o}", n.unsigned_abs()) } else { format!("0o{n:o}") };
            Ok(Some(Value::String(formatted.into())))
        }
        "hex" => {
            check_arg_count(name, args, 1, 1)?;
            let n = value_to_i64(&args[0])?;
            let formatted =
                if n < 0 { format!("-0x{:x}", n.unsigned_abs()) } else { format!("0x{n:x}") };
            Ok(Some(Value::String(formatted.into())))
        }
        "divmod" => {
            // CPython: divmod(a, b) -> (a // b, a % b) as a tuple. The
            // tuple ordering is the load-bearing parity property — most
            // user code unpacks it as `q, r = divmod(...)`.
            check_arg_count(name, args, 2, 2)?;
            let quotient = crate::types::dispatch_binop(
                crate::types::BinOp::FloorDiv,
                &args[0],
                &args[1],
                state.decimal_prec,
            )?;
            let remainder = crate::types::dispatch_binop(
                crate::types::BinOp::Mod,
                &args[0],
                &args[1],
                state.decimal_prec,
            )?;
            Ok(Some(Value::Tuple(vec![quotient, remainder])))
        }
        "id" => {
            check_arg_count(name, args, 1, 1)?;
            Ok(Some(Value::Int(object_id(&args[0]))))
        }
        "input" => Err(InterpreterError::Security(
            "input() is not allowed in sandboxed interpreter".into(),
        )
        .into()),
        _ => Ok(None),
    }
}
