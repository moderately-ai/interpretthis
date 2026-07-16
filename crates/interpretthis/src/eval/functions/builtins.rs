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
        BUILTIN_TYPE_NAMES, SortRequest, apply_key_fn, builtin_type_issubclass,
        bytes_from_int_items, check_isinstance, dsu_sort, object_id, parse_complex_str,
        parse_int_str, pow_three_arg, type_arg_name,
    },
    method_dispatch::CallArgs,
    resolve_proxy, round_bigint, round_decimal, round_float, round_fraction, round_int, to_len_i64,
    value_to_i64,
};
use crate::{
    error::{EvalError, InterpreterError},
    eval::literals::value_to_key,
    state::InterpreterState,
    tools::Tools,
    value::{ExceptionValue, LazyKind, Value, ValueKey, shared_list},
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
            | "RecursionError"
            | "ExceptionGroup"
            | "BaseExceptionGroup"
            | "GeneratorExit"
            | "StopAsyncIteration"
            | "KeyboardInterrupt"
            | "SystemExit"
            | "UnicodeError"
            | "UnicodeDecodeError"
            | "UnicodeEncodeError"
            | "FloatingPointError"
            | "BufferError"
            | "ReferenceError"
            | "MemoryError"
            | "PermissionError"
            | "NotADirectoryError"
            | "IsADirectoryError"
            | "FileExistsError"
            | "InterruptedError"
            | "ConnectionError"
            | "ConnectionAbortedError"
            | "ConnectionRefusedError"
            | "ConnectionResetError"
            | "ProcessLookupError"
            | "TimeoutError"
            | "BlockingIOError"
            | "ChildProcessError"
            | "BrokenPipeError"
            | "IndentationError"
            | "TabError"
            | "SyntaxError"
            | "SystemError"
            | "EOFError"
            | "ModuleNotFoundError"
            | "ImportError"
            | "UnboundLocalError"
            | "BaseException"
    )
}

/// Whether `value` is callable — every shape `call_value_as_function` accepts.
/// Drives both `callable(x)` and `hasattr(x, '__call__')` (CPython keeps them in
/// lockstep). An instance is callable iff its class MRO defines `__call__`; the
/// bare-name/method sentinels and class objects are always callable.
pub(super) fn value_is_callable(state: &InterpreterState, value: &Value) -> bool {
    match value {
        Value::Instance(inst) => state.classes.get(&inst.class_name).is_some_and(|class| {
            class.mro.iter().any(|anc| {
                state.classes.get(anc).is_some_and(|c| c.methods.contains_key("__call__"))
            })
        }),
        other => matches!(
            other,
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
        ),
    }
}

/// Wrap eagerly-computed `items` in a one-shot `Lazy` iterator with a fresh
/// cursor. CPython's `zip`/`map`/`filter`/`enumerate`/`reversed` return
/// single-use *iterators*, not lists: `next()` advances them, a second pass
/// sees only the remainder, and they are neither subscriptable nor sized. We
/// still compute the items up front (the sandbox caps iteration) but expose the
/// iterator protocol so those observable behaviours match.
fn into_iter_value(state: &mut InterpreterState, items: Vec<Value>, kind: LazyKind) -> Value {
    state.alloc_lazy_kind(items, kind)
}

/// `int(x=0, base=…)` — extracted from `try_builtin` so that hot funnel's
/// future stays small on the recursion path. The value is positional-only;
/// only `base` is a valid keyword.
async fn builtin_int(
    state: &mut InterpreterState,
    args: &[Value],
    kwargs: &IndexMap<String, Value>,
    tools: &Tools,
) -> Result<Option<Value>, EvalError> {
    check_arg_count("int", args, 0, 2)?;
    if let Some(bad) = kwargs.keys().find(|k| k.as_str() != "base") {
        return Err(InterpreterError::TypeError(format!(
            "'{bad}' is an invalid keyword argument for int()"
        ))
        .into());
    }
    if args.is_empty() && kwargs.is_empty() {
        return Ok(Some(Value::Int(0)));
    }
    // The base is `args[1]` or the `base` keyword (`int("101", base=2)`).
    let base_arg = args.get(1).cloned().or_else(|| kwargs.get("base").cloned());
    // An explicit base only applies to a string. CPython: `int(255, 16)`
    // raises TypeError, and any base outside {0} ∪ [2, 36] raises ValueError.
    if let Some(base_val) = base_arg {
        if args.is_empty() {
            return Err(InterpreterError::TypeError("int() missing string argument".into()).into());
        }
        let base = value_to_i64(&base_val)?;
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
    // A user class converts via __int__ (falling back to __index__ / __trunc__).
    if matches!(&args[0], Value::Instance(_)) {
        for slot in ["__int__", "__index__", "__trunc__"] {
            if let Some(result) =
                crate::eval::op::instance_unary_dunder(state, &args[0], slot, tools).await
            {
                return Ok(Some(result?));
            }
        }
    }
    match &args[0] {
        Value::Int(i) => Ok(Some(Value::Int(*i))),
        Value::BigInt(b) => Ok(Some(Value::BigInt(b.clone()))),
        Value::Float(f) => Ok(Some(float_to_int_exact(*f)?)),
        Value::Bool(b) => Ok(Some(Value::Int(i64::from(*b)))),
        Value::String(s) => Ok(Some(parse_int_str(s, 10)?)),
        // Decimal / Fraction truncate toward zero, as CPython's int() does.
        Value::Decimal(d, _) => {
            let (int_val, _) = d.with_scale(0).as_bigint_and_exponent();
            Ok(Some(crate::value::int_from_bigint(int_val)))
        }
        Value::Fraction(fr) => Ok(Some(crate::value::int_from_bigint(fr.to_integer()))),
        // IntEnum/IntFlag → underlying int; StrEnum → parse the underlying str.
        // A plain Enum/Flag is not a number (CPython raises TypeError).
        Value::EnumMember { value: inner, kind, .. } => match kind {
            crate::value::EnumKind::Int | crate::value::EnumKind::IntFlag => {
                crate::value::value_as_bigint(inner)
                    .map(crate::value::int_from_bigint)
                    .map(Some)
                    .ok_or_else(|| {
                        InterpreterError::TypeError(
                            "int() argument must be a string or a number".into(),
                        )
                        .into()
                    })
            }
            crate::value::EnumKind::Str => match &**inner {
                Value::String(s) => Ok(Some(parse_int_str(s, 10)?)),
                _ => Err(InterpreterError::TypeError(
                    "int() argument must be a string or a number".into(),
                )
                .into()),
            },
            crate::value::EnumKind::Plain | crate::value::EnumKind::Flag => Err(
                InterpreterError::TypeError("int() argument must be a string or a number".into())
                    .into(),
            ),
        },
        _ => Err(InterpreterError::TypeError(format!(
            "int() argument must be a string or a number, not '{}'",
            args[0].type_name()
        ))
        .into()),
    }
}

/// `ord()` over a bytes-like: a length-1 buffer yields its single byte as an
/// int; any other length raises CPython's "expected a character" TypeError.
fn ord_single_byte(bytes: &[u8]) -> Result<Option<Value>, EvalError> {
    if bytes.len() == 1 {
        Ok(Some(Value::Int(i64::from(bytes[0]))))
    } else {
        Err(InterpreterError::TypeError(format!(
            "ord() expected a character, but string of length {} found",
            bytes.len()
        ))
        .into())
    }
}

/// Remove Python's `_` numeric digit separators, returning `None` when one is
/// not flanked by two digits (CPython rejects `_1`, `1_`, `1__0`, `1_.0`).
fn clean_numeric_underscores(s: &str) -> Option<String> {
    if !s.contains('_') {
        return Some(s.to_string());
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            let prev_digit =
                i.checked_sub(1).and_then(|j| chars.get(j)).is_some_and(char::is_ascii_digit);
            let next_digit = chars.get(i + 1).is_some_and(|c| c.is_ascii_digit());
            if !(prev_digit && next_digit) {
                return None;
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
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
            let text = format!("{}{}", parts.join(&sep), end);
            // `file=` routes the output. Absent or `sys.stdout` → the capture
            // buffer; a StringIO → its buffer (reference-semantic); `sys.stderr`
            // → discarded from captured stdout, matching CPython's separate
            // stderr stream. `flush=` is accepted and ignored (we don't buffer).
            match kwargs.get("file") {
                Some(Value::StringIO(stream)) => {
                    crate::eval::functions::methods::stringio::write_string(stream, &text);
                }
                Some(Value::Type(sentinel))
                    if sentinel == crate::eval::modules::sys_mod::STDERR_SENTINEL =>
                {
                    // stderr: not part of captured stdout.
                }
                _ => {
                    state.append_print(&text).map_err(EvalError::Interpreter)?;
                }
            }
            Ok(Some(Value::None))
        }
        "len" => {
            check_arg_count(name, args, 1, 1)?;
            let length = crate::eval::op::len(state, &args[0], tools).await?;
            Ok(Some(Value::Int(to_len_i64(length)?)))
        }
        "range" => {
            // range bounds accept anything with __index__, coerced to i64 first.
            let mut c = Vec::with_capacity(args.len());
            for a in args {
                c.push(value_to_i64(
                    &crate::eval::op::coerce_index(state, a.clone(), tools).await?,
                )?);
            }
            let (start, stop, stride) = match c.len() {
                1 => (0, c[0], 1),
                2 => (c[0], c[1], 1),
                3 => (c[0], c[1], c[2]),
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
            // The decoding form `str(bytes, encoding[, errors])` — encoding
            // given positionally or by keyword — decodes a bytes-like object,
            // exactly like `bytes.decode(encoding, errors)`. Without an encoding
            // it is the ordinary object-to-string conversion.
            let encoding = args.get(1).cloned().or_else(|| kwargs.get("encoding").cloned());
            let errors = args.get(2).cloned().or_else(|| kwargs.get("errors").cloned());
            if encoding.is_some() || errors.is_some() {
                check_arg_count(name, args, 1, 3)?;
                let raw = match &args[0] {
                    Value::Bytes(b) => b.clone(),
                    Value::ByteArray(b) => b.lock().clone(),
                    other => {
                        return Err(InterpreterError::TypeError(format!(
                            "decoding to str: need a bytes-like object, {} found",
                            other.type_name()
                        ))
                        .into());
                    }
                };
                let mut decode_args =
                    vec![encoding.unwrap_or_else(|| Value::String("utf-8".into()))];
                if let Some(err) = errors {
                    decode_args.push(err);
                }
                return crate::eval::functions::methods::bytes::dispatch_bytes_method(
                    &raw,
                    "decode",
                    &decode_args,
                    &IndexMap::new(),
                )
                .map(Some);
            }
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
        // Extracted into a helper so this hot `try_builtin` future (embedded in
        // `eval_call` on the recursion path) stays small — see the recursion
        // canary note in the productive-probe-areas memory.
        "int" => builtin_int(state, args, kwargs, tools).await,
        "float" => {
            check_arg_count(name, args, 0, 1)?;
            if args.is_empty() {
                return Ok(Some(Value::Float(0.0)));
            }
            // A user class converts via __float__ (falling back to __index__),
            // so float(instance) dispatches before the numeric arms — mirroring
            // int()/__int__ above.
            if matches!(&args[0], Value::Instance(_)) {
                for slot in ["__float__", "__index__"] {
                    if let Some(result) =
                        crate::eval::op::instance_unary_dunder(state, &args[0], slot, tools).await
                    {
                        let v = result?;
                        return Ok(Some(Value::Float(v.as_float().ok_or_else(|| {
                            EvalError::from(InterpreterError::TypeError(format!(
                                "{slot} returned non-float (type {})",
                                v.type_name()
                            )))
                        })?)));
                    }
                }
            }
            match &args[0] {
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
                    let float_err = || {
                        EvalError::Exception(ExceptionValue::new(
                            "ValueError",
                            format!("could not convert string to float: '{trimmed}'"),
                        ))
                    };
                    // CPython accepts `_` digit separators (`float("1_000.5")`),
                    // but only between two digits.
                    let cleaned = clean_numeric_underscores(trimmed).ok_or_else(float_err)?;
                    let f = cleaned.parse::<f64>().map_err(|_| float_err())?;
                    Ok(Some(Value::Float(f)))
                }
                // int/float/bool/Decimal/Fraction/BigInt all convert via the
                // numeric-tower accessor.
                other => other.as_float().map(Value::Float).map(Some).ok_or_else(|| {
                    InterpreterError::TypeError(format!(
                        "float() argument must be a string or a number, not '{}'",
                        other.type_name()
                    ))
                    .into()
                }),
            }
        }
        "complex" => {
            use num_complex::Complex64;
            check_arg_count(name, args, 0, 2)?;
            let malformed = || {
                EvalError::Exception(ExceptionValue::new(
                    "ValueError",
                    "complex() arg is a malformed string",
                ))
            };
            // `real`/`imag` may be passed positionally OR by keyword
            // (`complex(real=3, imag=4)`, `complex(imag=5)`).
            for k in kwargs.keys() {
                if k != "real" && k != "imag" {
                    return Err(InterpreterError::TypeError(format!(
                        "'{k}' is an invalid keyword argument for complex()"
                    ))
                    .into());
                }
            }
            let real_arg = args.first().or_else(|| kwargs.get("real"));
            let imag_arg = args.get(1).or_else(|| kwargs.get("imag"));
            let result = match (real_arg, imag_arg) {
                (None, None) => Complex64::new(0.0, 0.0),
                // A single string argument is parsed as a complex literal.
                (Some(Value::String(s)), None) => parse_complex_str(s).ok_or_else(malformed)?,
                (Some(Value::String(_)), Some(_)) => {
                    return Err(InterpreterError::TypeError(
                        "complex() can't take second arg if first is a string".into(),
                    )
                    .into());
                }
                (Some(re), None) => crate::types::value_to_complex(re).ok_or_else(|| {
                    EvalError::from(InterpreterError::TypeError(format!(
                        "complex() first argument must be a string or a number, not '{}'",
                        re.type_name()
                    )))
                })?,
                (real_arg, Some(im)) => {
                    if matches!(im, Value::String(_)) {
                        return Err(InterpreterError::TypeError(
                            "complex() second arg can't be a string".into(),
                        )
                        .into());
                    }
                    let a = match real_arg {
                        None => Complex64::new(0.0, 0.0),
                        Some(re) => crate::types::value_to_complex(re).ok_or_else(|| {
                            EvalError::from(InterpreterError::TypeError(
                                "complex() first argument must be a string or a number".into(),
                            ))
                        })?,
                    };
                    let b = crate::types::value_to_complex(im).ok_or_else(|| {
                        EvalError::from(InterpreterError::TypeError(
                            "complex() second argument must be a number".into(),
                        ))
                    })?;
                    // complex(real, imag) == real + imag*1j (both may be complex).
                    a + b * Complex64::new(0.0, 1.0)
                }
            };
            Ok(Some(Value::Complex(Box::new(result))))
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
                // An enum member's type is its enum class, so
                // `type(Color.RED).__name__ == 'Color'` and
                // `type(Color.RED) is Color`.
                Value::EnumMember { class_name, .. } => Value::Class(class_name.clone()),
                Value::Type(_) | Value::Class(_) | Value::ExceptionType(_) => {
                    Value::Type("type".to_string())
                }
                // A bare builtin *type* name (`int`, `str`, `list`) is itself a
                // type object, so its type is `type`. A builtin *function*
                // (`len`, `print`) falls through to `builtin_function_or_method`.
                Value::BuiltinName(n) if crate::value::is_builtin_type_name(n) => {
                    Value::Type("type".to_string())
                }
                Value::Module(_) => Value::Type("module".to_string()),
                other => Value::Type(other.type_name().to_string()),
            };
            Ok(Some(type_obj))
        }
        // Internal helper produced by functools.wraps — stamps the captured
        // name onto the decorated function's `__name__`. Not user-callable
        // (the name doesn't resolve as a bare builtin).
        "__apply_wraps__" => {
            let [Value::String(new_name), new_doc, target] = args else {
                return Err(InterpreterError::TypeError(
                    "__apply_wraps__ expects (name, doc, function)".into(),
                )
                .into());
            };
            match target {
                Value::Function(fd) => {
                    let mut renamed = (**fd).clone();
                    renamed.wraps_name = Some(new_name.to_string());
                    // wraps copies __doc__ (overwriting the wrapper's own).
                    renamed.docstring = match new_doc {
                        Value::String(s) => Some(s.to_string()),
                        _ => None,
                    };
                    Ok(Some(Value::Function(std::sync::Arc::new(renamed))))
                }
                // Non-function targets pass through unchanged.
                other => Ok(Some(other.clone())),
            }
        }
        // Internal: `@contextmanager`'s Partial dispatches here. args[0]
        // is the decorated generator function, args[1..] the call args.
        // Run the generator and box it in a `_GeneratorContextManager`.
        "__gen_contextmanager__" => {
            let Some((func, call_args)) = args.split_first() else {
                return Err(InterpreterError::TypeError(
                    "@contextmanager wrapper called without its function".into(),
                )
                .into());
            };
            let generator = call_value_as_function(state, func, call_args, kwargs, tools).await?;
            Ok(Some(crate::eval::modules::contextlib_mod::wrap_generator_cm(state, generator)))
        }
        "memoryview" => {
            check_arg_count(name, args, 1, 1)?;
            match &args[0] {
                Value::Bytes(_) | Value::ByteArray(_) => {
                    Ok(Some(Value::MemoryView(Box::new(args[0].clone()))))
                }
                Value::MemoryView(_) => Ok(Some(args[0].clone())),
                other => Err(InterpreterError::TypeError(format!(
                    "memoryview: a bytes-like object is required, not '{}'",
                    other.type_name()
                ))
                .into()),
            }
        }
        "slice" => {
            // slice(stop) or slice(start, stop[, step]). Each bound is an int
            // or None; bool folds to int (bool is an int subclass).
            check_arg_count(name, args, 1, 3)?;
            let bound = |v: &Value| -> Result<Value, EvalError> {
                match v {
                    Value::None | Value::Int(_) | Value::BigInt(_) => Ok(v.clone()),
                    Value::Bool(b) => Ok(Value::Int(i64::from(*b))),
                    _ => Err(InterpreterError::TypeError(
                        "slice indices must be integers or None or have an __index__ method"
                            .to_string(),
                    )
                    .into()),
                }
            };
            let (start, stop, step) = match args {
                [stop] => (Value::None, bound(stop)?, Value::None),
                [start, stop] => (bound(start)?, bound(stop)?, Value::None),
                [start, stop, step] => (bound(start)?, bound(stop)?, bound(step)?),
                _ => unreachable!("check_arg_count bounds args to 1..=3"),
            };
            Ok(Some(Value::Slice(Box::new(crate::value::SliceValue { start, stop, step }))))
        }
        "object" => {
            // Bare `object()` — CPython's universal base. Takes no arguments
            // and yields a fresh identity, supporting the common sentinel
            // idiom `_MISSING = object()`. Identity/equality/hash all key on
            // the instance's shared-fields Arc.
            check_arg_count(name, args, 0, 0)?;
            if !kwargs.is_empty() {
                return Err(InterpreterError::TypeError(
                    "object() takes no keyword arguments".into(),
                )
                .into());
            }
            Ok(Some(Value::Instance(crate::value::InstanceValue {
                class_name: "object".into(),
                fields: crate::value::shared_fields(std::collections::BTreeMap::new()),
            })))
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
                    match &live_self {
                        Value::Instance(inst) => Ok(Some(Value::Super {
                            defining_class,
                            instance: Box::new(inst.clone()),
                        })),
                        // Classmethod context (`cls` receiver), e.g. inside
                        // `__init_subclass__`: a class-bound super proxy.
                        Value::Class(class_name) => Ok(Some(Value::SuperClass {
                            defining_class,
                            class_name: class_name.clone(),
                        })),
                        _ => Err(InterpreterError::Runtime(
                            "super(): current self is not an instance".into(),
                        )
                        .into()),
                    }
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
            let child_name = match &args[0] {
                Value::Class(n) | Value::Type(n) | Value::ExceptionType(n) => n.clone(),
                // A bare builtin name is a class only if it names a type
                // (`bool`), not a function (`len`).
                Value::BuiltinName(n) if BUILTIN_TYPE_NAMES.contains(&n.as_str()) => n.clone(),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "issubclass() arg 1 must be a class".into(),
                    )
                    .into());
                }
            };
            let class = state.classes.get(&child_name);
            let check_one = |target_name: &str| -> bool {
                if let Some(c) = class {
                    if target_name == "object" || c.mro.iter().any(|a| a == target_name) {
                        return true;
                    }
                }
                builtin_type_issubclass(&child_name, target_name)
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
            let has_default = args.len() >= 3;
            match crate::eval::names::getattr_on_value(state, obj, attr_name, tools, None).await {
                Ok(v) => Ok(Some(v)),
                // Default only swallows AttributeError — Security on blocked
                // dunders stays a hard failure. A user `__getattr__` that raises
                // `AttributeError` surfaces as an Exception value, so catch that
                // (and subclasses) too, not just the interpreter-internal miss.
                Err(EvalError::Interpreter(InterpreterError::AttributeError(_))) if has_default => {
                    Ok(Some(args[2].clone()))
                }
                Err(EvalError::Exception(ref e))
                    if has_default
                        && (e.type_name == "AttributeError"
                            || crate::eval::exceptions::builtin_exception_issubclass(
                                &e.type_name,
                                "AttributeError",
                            )) =>
                {
                    Ok(Some(args[2].clone()))
                }
                Err(e) => Err(e),
            }
        }
        "vars" => {
            // Bounded, instance-only. `vars(obj)` returns a *copy* of the
            // instance's fields — every value is already reachable via
            // getattr(obj, name), and field keys provably can never be a blocked
            // dunder (the attribute-write paths validate names), so this exposes
            // nothing new. Keys are re-filtered through validate_attribute as
            // defence-in-depth against any future unguarded field-write path.
            //
            // Deliberately NARROWER than CPython: the no-arg form (== locals()),
            // and the module / class / type forms are rejected. Those would
            // re-expose scope bindings or the class-walk chain (bases / mro /
            // methods) that BLOCKED_ATTRIBUTES exists to hide. locals/globals
            // stay on the security denylist.
            check_arg_count(name, args, 0, 1)?;
            let Some(arg) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "vars() with no arguments is not supported in the sandboxed interpreter (locals() is blocked)".into(),
                )
                .into());
            };
            let obj = resolve_proxy(arg).await?;
            let Value::Instance(inst) = obj else {
                return Err(InterpreterError::TypeError(
                    "vars() argument must have __dict__ attribute".into(),
                )
                .into());
            };
            let snapshot = inst.fields.lock().clone();
            let mut map = indexmap::IndexMap::new();
            for (k, v) in snapshot {
                if crate::security::validator::validate_attribute(&k).is_err() {
                    continue;
                }
                map.insert(crate::value::ValueKey::String(k.as_str().into()), v);
            }
            Ok(Some(Value::Dict(crate::value::shared_dict(map))))
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
            // `hasattr(x, '__call__')` mirrors `callable(x)` — every callable
            // exposes a `__call__`, and no non-callable does.
            if attr_name == "__call__" {
                return Ok(Some(Value::Bool(value_is_callable(state, &args[0]))));
            }
            // An instance whose class overrides attribute access
            // (`__getattr__` / `__getattribute__`) can resolve names the static
            // lookup below misses. Mirror CPython by running the real lookup and
            // reporting whether it raised AttributeError (any other exception
            // propagates, as in CPython 3).
            if let Value::Instance(inst) = &args[0] {
                let overrides = crate::eval::classes::lookup_method_in_mro(
                    state,
                    &inst.class_name,
                    "__getattr__",
                )
                .is_some()
                    || crate::eval::classes::lookup_method_in_mro(
                        state,
                        &inst.class_name,
                        "__getattribute__",
                    )
                    .is_some();
                if overrides {
                    let resolved = crate::eval::names::getattr_on_value(
                        state,
                        args[0].clone(),
                        attr_name,
                        tools,
                        None,
                    )
                    .await;
                    return Ok(Some(Value::Bool(match resolved {
                        Ok(_) => true,
                        // Internal miss, or a user `raise AttributeError` (which
                        // surfaces as an Exception value) / a subclass thereof.
                        Err(EvalError::Interpreter(InterpreterError::AttributeError(_))) => false,
                        Err(EvalError::Exception(ref e))
                            if e.type_name == "AttributeError"
                                || crate::eval::exceptions::builtin_exception_issubclass(
                                    &e.type_name,
                                    "AttributeError",
                                ) =>
                        {
                            false
                        }
                        Err(e) => return Err(e),
                    })));
                }
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
                    Value::Function(func_def) => {
                        attr_name == "__name__"
                            || attr_name == "__qualname__"
                            || attr_name == "__doc__"
                            || attr_name == "__annotations__"
                            || attr_name == "__call__"
                            || attr_name == "__defaults__"
                            || attr_name == "__kwdefaults__"
                            || state
                                .function_attrs
                                .get(func_def.body_cache_key())
                                .is_some_and(|m| m.contains_key(attr_name))
                    }
                    // A builtin type object (`str`, `float`, ...) exposes its
                    // methods and dunders as unbound descriptors, so
                    // `hasattr(str, "upper")` is True — mirror the resolution
                    // `getattr` uses via the shared attribute registry.
                    Value::BuiltinName(type_name) | Value::Type(type_name) => {
                        crate::types::builtin_type_attr_present(type_name, attr_name)
                    }
                    Value::Lambda(_) => attr_name == "__name__" || attr_name == "__qualname__",
                    Value::Module(module) => {
                        crate::eval::modules::module_member(module, attr_name).is_ok()
                    }
                    Value::Date(_) => {
                        matches!(attr_name, "year" | "month" | "day" | "isoformat" | "weekday")
                    }
                    // Keep in sync with the introspection arms in
                    // `names::legacy_attribute`.
                    Value::LruCache(_) => {
                        matches!(attr_name, "__name__" | "__qualname__" | "__doc__" | "__wrapped__")
                    }
                    Value::BoundMethod { .. } => {
                        matches!(attr_name, "__name__" | "__qualname__" | "__self__")
                    }
                    Value::BuiltinTypeMethod { .. } => {
                        matches!(attr_name, "__name__" | "__qualname__")
                    }
                    _ => false,
                },
                Err(_) => false,
            };
            Ok(Some(Value::Bool(has)))
        }
        "callable" => {
            check_arg_count(name, args, 1, 1)?;
            Ok(Some(Value::Bool(value_is_callable(state, &args[0]))))
        }
        "abs" => {
            check_arg_count(name, args, 1, 1)?;
            if let Some(result) =
                crate::eval::op::instance_unary_dunder(state, &args[0], "__abs__", tools).await
            {
                return Ok(Some(result?));
            }
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
                // abs(complex) is the magnitude (a float): sqrt(re^2 + im^2).
                Value::Complex(c) => Ok(Some(Value::Float(c.norm()))),
                Value::Bool(b) => Ok(Some(Value::Int(i64::from(*b)))),
                // `abs(timedelta)` yields a non-negative duration.
                Value::TimeDelta(us) => Ok(Some(Value::TimeDelta(us.abs()))),
                Value::Decimal(d, k) => Ok(Some(crate::eval::modules::decimal::abs_decimal(d, *k))),
                Value::Fraction(fr) => {
                    use num_traits::Signed as _;
                    Ok(Some(Value::Fraction(Box::new((**fr).abs()))))
                }
                // IntEnum / IntFlag members take abs on their underlying int; a
                // plain Enum / Flag is not numeric (CPython raises TypeError).
                Value::EnumMember {
                    value: inner,
                    kind: crate::value::EnumKind::Int | crate::value::EnumKind::IntFlag,
                    ..
                } => {
                    use num_traits::Signed as _;
                    crate::value::value_as_bigint(inner)
                        .map(|b| Some(crate::value::int_from_bigint(b.abs())))
                        .ok_or_else(|| {
                            InterpreterError::TypeError(format!(
                                "bad operand type for abs(): '{}'",
                                args[0].type_name()
                            ))
                            .into()
                        })
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
            // `ndigits` may be positional or a keyword (`round(x, ndigits=2)`).
            let ndigits_arg = args.get(1).or_else(|| kwargs.get("ndigits"));
            // A user-class instance rounds through its own `__round__`.
            if let Some(result) =
                crate::eval::op::instance_round_dunder(state, &args[0], ndigits_arg, tools).await
            {
                return Ok(Some(result?));
            }
            let ndigits = match ndigits_arg {
                Some(Value::None) | None => None,
                Some(v) => Some(value_to_i64(v)?),
            };
            match &args[0] {
                // CPython's `round(int, n)` returns an int rounded to the
                // nearest multiple of 10**(-n) for n<0; rounding is
                // banker's. `round(int)` and `round(int, n>=0)` are no-ops.
                Value::Int(i) => Ok(Some(round_int(*i, ndigits))),
                Value::BigInt(b) => {
                    Ok(Some(crate::value::int_from_bigint(round_bigint(b, ndigits))))
                }
                // CPython's `round()` uses IEEE-754 round-half-to-even
                // (banker's rounding): `round(0.5) == 0`, `round(2.5) == 2`,
                // `round(-0.5) == 0`. Rust's `f64::round()` is
                // round-half-away-from-zero — wrong for parity. Use
                // `round_ties_even()` which implements the IEEE rule.
                Value::Float(f) => Ok(Some(round_float(*f, ndigits)?)),
                Value::Bool(b) => Ok(Some(Value::Int(i64::from(*b)))),
                Value::Decimal(d, _) => Ok(Some(round_decimal(d, ndigits))),
                Value::Fraction(fr) => Ok(Some(round_fraction(fr, ndigits))),
                // round(IntEnum/IntFlag) rounds the underlying int (a no-op for
                // ndigits >= 0). Plain Enum/Flag has no __round__, so it errors.
                Value::EnumMember {
                    value: inner,
                    kind: crate::value::EnumKind::Int | crate::value::EnumKind::IntFlag,
                    ..
                } => match &**inner {
                    Value::Int(i) => Ok(Some(round_int(*i, ndigits))),
                    Value::BigInt(b) => {
                        Ok(Some(crate::value::int_from_bigint(round_bigint(b, ndigits))))
                    }
                    Value::Bool(b) => Ok(Some(Value::Int(i64::from(*b)))),
                    _ => Err(InterpreterError::TypeError(format!(
                        "type '{}' doesn't define __round__",
                        args[0].type_name()
                    ))
                    .into()),
                },
                _ => Err(InterpreterError::TypeError(format!(
                    "type '{}' doesn't define __round__",
                    args[0].type_name()
                ))
                .into()),
            }
        }
        "min" => {
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "min expected at least 1 argument, got 0".into(),
                )
                .into());
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
                return Err(InterpreterError::ValueError(
                    "min() iterable argument is empty".into(),
                )
                .into());
            }
            let key_fn = kwargs.get("key");
            let mut min_val = items[0].clone();
            let mut min_key = apply_key_fn(state, &min_val, key_fn, tools).await?;
            for item in items.iter().skip(1) {
                let item_key = apply_key_fn(state, item, key_fn, tools).await?;
                // Async `op::lt` so instance keys dispatch through `__lt__`.
                if crate::eval::op::lt(state, &item_key, &min_key, tools).await? {
                    min_val = item.clone();
                    min_key = item_key;
                }
            }
            Ok(Some(min_val))
        }
        "max" => {
            if args.is_empty() {
                return Err(InterpreterError::TypeError(
                    "max expected at least 1 argument, got 0".into(),
                )
                .into());
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
                return Err(InterpreterError::ValueError(
                    "max() iterable argument is empty".into(),
                )
                .into());
            }
            let key_fn = kwargs.get("key");
            let mut max_val = items[0].clone();
            let mut max_key = apply_key_fn(state, &max_val, key_fn, tools).await?;
            for item in items.iter().skip(1) {
                let item_key = apply_key_fn(state, item, key_fn, tools).await?;
                // Async `op::lt` so instance keys dispatch through `__lt__`.
                if crate::eval::op::lt(state, &max_key, &item_key, tools).await? {
                    max_val = item.clone();
                    max_key = item_key;
                }
            }
            Ok(Some(max_val))
        }
        "sum" => {
            check_arg_count(name, args, 1, 2)?;
            let items = crate::eval::op::iter(state, &args[0], tools).await?;
            // `start` is the second positional or the `start=` keyword (CPython
            // 3.12 accepts both), defaulting to 0.
            let start = if args.len() >= 2 {
                args[1].clone()
            } else {
                kwargs.get("start").cloned().unwrap_or(Value::Int(0))
            };
            // CPython 3.12 sums a run of floats with Neumaier compensation,
            // so `sum([0.1] * 10) == 1.0` rather than 0.999…. Take that
            // fast path when start + every item is a plain number and at
            // least one is a float; otherwise fall back to repeated
            // `__add__` (which stays exact for pure-int sums and handles
            // list concatenation, custom types, etc.).
            if let Some(result) = sum_float_neumaier(&start, &items) {
                return Ok(Some(result));
            }
            let mut total = start;
            for item in items {
                // Use the async operator path so a custom start / element type
                // dispatches through `__add__`/`__radd__` (e.g. summing a list
                // of instances, where `0 + obj` needs the reflected slot).
                total =
                    crate::eval::op::binop(state, ast::Operator::Add, &total, &item, tools).await?;
            }
            Ok(Some(total))
        }
        "all" | "any" => {
            check_arg_count(name, args, 1, 1)?;
            // `any`/`all` short-circuit — over a lazy iterator they must step it
            // one item at a time (so `any(map(pred, count()))` stops at the first
            // truthy element instead of hanging while materialising the source).
            let want_any = name == "any";
            if matches!(
                &args[0],
                Value::Generator { .. } | Value::Lazy { .. } | Value::BuiltinIter { .. }
            ) {
                let empty = IndexMap::new();
                loop {
                    let item = match crate::eval::functions::dispatch_generator_method(
                        state,
                        &args[0],
                        "__next__",
                        &[],
                        &empty,
                        tools,
                    )
                    .await
                    {
                        Ok(v) => v,
                        Err(EvalError::Exception(e)) if e.type_name == "StopIteration" => break,
                        Err(e) => return Err(e),
                    };
                    if crate::eval::op::truthy(state, &item, tools).await? == want_any {
                        return Ok(Some(Value::Bool(want_any)));
                    }
                }
                return Ok(Some(Value::Bool(!want_any)));
            }
            let items = crate::eval::op::iter(state, &args[0], tools).await?;
            // Async truthiness so instance elements dispatch __bool__/__len__.
            for item in &items {
                if crate::eval::op::truthy(state, item, tools).await? == want_any {
                    return Ok(Some(Value::Bool(want_any)));
                }
            }
            Ok(Some(Value::Bool(!want_any)))
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
            // `start` accepted positionally OR via keyword.
            let start = if args.len() >= 2 {
                value_to_i64(&args[1])?
            } else if let Some(s) = kwargs.get("start") {
                value_to_i64(s)?
            } else {
                0
            };
            // Over an infinite producer, enumerate is lazy (CPython's `enumerate`
            // object) — `next(enumerate(count()))` streams, not hangs.
            if has_infinite_arg(&args[..1]) {
                if let Some(g) = lazy_gen_builtin(
                    state,
                    "<enumerate>",
                    "i = start\nfor x in it:\n    yield (i, x)\n    i = i + 1\n",
                    &[("it", args[0].clone()), ("start", Value::Int(start))],
                ) {
                    return Ok(Some(g));
                }
            }
            let items = crate::eval::op::iter(state, &args[0], tools).await?;
            let mut result = Vec::with_capacity(items.len());
            for (i, v) in items.into_iter().enumerate() {
                result.push(Value::Tuple(vec![Value::Int(start + to_len_i64(i)?), v]));
            }
            Ok(Some(into_iter_value(state, result, LazyKind::Enumerate)))
        }
        "zip" => {
            if args.is_empty() {
                return Ok(Some(into_iter_value(state, Vec::new(), LazyKind::Zip)));
            }
            let strict = kwargs.get("strict").is_some_and(Value::is_truthy);
            // If any argument is an infinite producer (count/cycle/repeat), zip
            // must be lazy — otherwise `zip(count(), "abc")` or the all-infinite
            // `zip(count(), count())` hangs. Return a real generator that pulls
            // one row at a time (stopping when any argument is exhausted), so a
            // downstream `islice`/`for`/`next` bounds it. `strict=` with an
            // infinite argument is undefined (nothing exhausts), so it uses the
            // same lazy path.
            if args.iter().any(|a| matches!(a, Value::BuiltinIter { .. })) {
                if let Some(g) = zip_lazy_gen(state, args) {
                    return Ok(Some(g));
                }
                return zip_lazy(state, args, tools).await.map(Some);
            }
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
            Ok(Some(into_iter_value(state, result, LazyKind::Zip)))
        }
        "reversed" => {
            check_arg_count(name, args, 1, 1)?;
            // A user instance is reversible when it defines `__reversed__` (call
            // it, return its iterator directly) or the `__len__` + `__getitem__`
            // sequence protocol (reverse-index it). An instance with only
            // `__iter__` is iterable but NOT reversible — CPython raises.
            if let Value::Instance(inst) = &args[0] {
                let class_name = inst.class_name.clone();
                if let Some((_, method)) =
                    crate::eval::classes::lookup_method_in_mro(state, &class_name, "__reversed__")
                {
                    let call = CallArgs { positional: &[], keyword: &IndexMap::new() };
                    let (result, _self) = crate::eval::classes::call_method(
                        state,
                        &method,
                        args[0].clone(),
                        call,
                        tools,
                    )
                    .await?;
                    return Ok(Some(result));
                }
                let getitem =
                    crate::eval::classes::lookup_method_in_mro(state, &class_name, "__getitem__");
                let len_method =
                    crate::eval::classes::lookup_method_in_mro(state, &class_name, "__len__");
                let (Some((_, getitem)), Some((_, len_method))) = (getitem, len_method) else {
                    return Err(InterpreterError::TypeError(format!(
                        "'{class_name}' object is not reversible"
                    ))
                    .into());
                };
                // CPython's sequence reverse-iterator indexes `len-1 .. 0` via
                // `__getitem__`; it relies on `__len__` for the bound rather than
                // iterating forward (a `__getitem__` that never raises IndexError
                // would otherwise never terminate).
                let empty = IndexMap::new();
                let (len_val, _self) = crate::eval::classes::call_method(
                    state,
                    &len_method,
                    args[0].clone(),
                    CallArgs { positional: &[], keyword: &empty },
                    tools,
                )
                .await?;
                let n = value_to_i64(&len_val)?;
                let mut items = Vec::with_capacity(usize::try_from(n.max(0)).unwrap_or(0));
                for i in (0..n).rev() {
                    let (item, _self) = crate::eval::classes::call_method(
                        state,
                        &getitem,
                        args[0].clone(),
                        CallArgs { positional: &[Value::Int(i)], keyword: &empty },
                        tools,
                    )
                    .await?;
                    items.push(item);
                }
                return Ok(Some(into_iter_value(state, items, LazyKind::Reversed)));
            }
            // `reversed` is stricter than `iter`: it needs a reversible sequence,
            // not just any iterable — a set/frozenset/generator is iterable but
            // NOT reversible.
            let reversible = matches!(
                &args[0],
                Value::List(_)
                    | Value::Tuple(_)
                    | Value::String(_)
                    | Value::Range { .. }
                    | Value::Bytes(_)
                    | Value::ByteArray(_)
                    | Value::Array { .. }
                    | Value::Dict(_)
                    | Value::OrderedDict(_)
                    | Value::DictView { .. }
            );
            if !reversible {
                return Err(InterpreterError::TypeError(format!(
                    "'{}' object is not reversible",
                    args[0].type_name()
                ))
                .into());
            }
            let mut items = crate::eval::op::iter(state, &args[0], tools).await?;
            items.reverse();
            // CPython names the reverse-iterator by source type: `list` ->
            // `list_reverseiterator`, `range` -> `range_iterator`, `dict` ->
            // `dict_reversekeyiterator`, and the other sequences -> the generic
            // `reversed`.
            let kind = match &args[0] {
                Value::List(_) => LazyKind::ListReverseIterator,
                Value::Range { .. } => LazyKind::RangeIterator,
                Value::Dict(_) | Value::OrderedDict(_) => LazyKind::DictReverseKeyIterator,
                Value::DictView { kind, .. } => match kind {
                    crate::value::DictViewKind::Keys => LazyKind::DictReverseKeyIterator,
                    crate::value::DictViewKind::Values => LazyKind::DictReverseValueIterator,
                    crate::value::DictViewKind::Items => LazyKind::DictReverseItemIterator,
                },
                Value::Tuple(_)
                | Value::String(_)
                | Value::Bytes(_)
                | Value::ByteArray(_)
                | Value::Array { .. } => LazyKind::Reversed,
                _ => LazyKind::Generator,
            };
            Ok(Some(into_iter_value(state, items, kind)))
        }
        "chr" => {
            check_arg_count(name, args, 1, 1)?;
            let code =
                value_to_i64(&crate::eval::op::coerce_index(state, args[0].clone(), tools).await?)?;
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
            match &args[0] {
                Value::String(s) => {
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
                }
                // A length-1 bytes/bytearray yields its single byte value.
                Value::Bytes(b) => ord_single_byte(b),
                Value::ByteArray(b) => ord_single_byte(&b.lock()),
                other => Err(InterpreterError::TypeError(format!(
                    "ord() expected string of length 1, but {} found",
                    other.type_name()
                ))
                .into()),
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
                return Ok(Some(Value::Dict(crate::value::shared_dict(IndexMap::new()))));
            }
            let mut map = IndexMap::new();
            // `dict(mapping)` copies key→value; only a non-mapping argument is
            // read as an iterable of pairs. Iterating a mapping yields its keys,
            // so the pairs path below would wrongly reject it. Snapshot the
            // source's contents (Dict is behind a lock; Counter/DefaultDict
            // store an IndexMap by value).
            let mapping_src: Option<IndexMap<ValueKey, Value>> = match args.first() {
                Some(Value::Dict(src) | Value::OrderedDict(src)) => Some(src.lock().clone()),
                Some(Value::Counter(src)) => Some(src.clone()),
                Some(Value::DefaultDict(data)) => Some(data.items.clone()),
                Some(Value::ChainMap(maps)) => Some(crate::types::chainmap_contents(maps)),
                _ => None,
            };
            if let Some(src) = mapping_src {
                for (k, v) in &src {
                    map.insert(k.clone(), v.clone());
                }
                for (k, v) in kwargs {
                    map.insert(ValueKey::String(k.clone().into()), v.clone());
                }
                return Ok(Some(Value::Dict(crate::value::shared_dict(map))));
            }
            if !args.is_empty() {
                // dict from iterable of pairs
                let items = crate::eval::op::iter(state, &args[0], tools).await?;
                // CPython reports the offending element's position and length:
                // a wrong-length pair is a ValueError, a non-sequence element a
                // TypeError, each carrying `element #<i>`.
                let wrong_len = |i: usize, len: usize| -> EvalError {
                    InterpreterError::ValueError(format!(
                        "dictionary update sequence element #{i} has length {len}; 2 is required"
                    ))
                    .into()
                };
                for (i, item) in items.into_iter().enumerate() {
                    if let Value::Tuple(pair) = &item {
                        if pair.len() == 2 {
                            let key = value_to_key(&pair[0])?;
                            map.insert(key, pair[1].clone());
                        } else {
                            return Err(wrong_len(i, pair.len()));
                        }
                    } else if let Value::List(pair) = &item {
                        // Snapshot the pair so the lock guard's scope
                        // doesn't span the value_to_key error path.
                        let snapshot = pair.lock().clone();
                        if snapshot.len() == 2 {
                            let key = value_to_key(&snapshot[0])?;
                            map.insert(key, snapshot[1].clone());
                        } else {
                            return Err(wrong_len(i, snapshot.len()));
                        }
                    } else {
                        return Err(InterpreterError::TypeError(format!(
                            "cannot convert dictionary update sequence element #{i} to a sequence"
                        ))
                        .into());
                    }
                }
            }
            for (k, v) in kwargs {
                map.insert(ValueKey::String(k.clone().into()), v.clone());
            }
            Ok(Some(Value::Dict(crate::value::shared_dict(map))))
        }
        "set" => {
            check_arg_count(name, args, 0, 1)?;
            if args.is_empty() {
                return Ok(Some(Value::new_set(Vec::new())));
            }
            let items = crate::eval::op::iter(state, &args[0], tools).await?;
            // Shared set construction: raises on an unhashable element and
            // dedups instances by __eq__ (both of which the old open-coded
            // `value_to_key(x).ok()` dedup got wrong — it silently included
            // unhashables and collapsed every instance to one).
            Ok(Some(crate::eval::literals::build_set(state, items, false, tools).await?))
        }
        "frozenset" => {
            check_arg_count(name, args, 0, 1)?;
            if args.is_empty() {
                return Ok(Some(Value::new_frozenset(Vec::new())));
            }
            let items = crate::eval::op::iter(state, &args[0], tools).await?;
            // Reuse the set builder (dedup + unhashable rejection), then freeze:
            // the SetBody (table + order) carries straight over into the Arc.
            let built = crate::eval::literals::build_set(state, items, false, tools).await?;
            let Value::Set(elements) = built else {
                unreachable!("build_set always returns Value::Set")
            };
            Ok(Some(Value::Frozenset(std::sync::Arc::new(elements.lock().clone()))))
        }
        "iter" => {
            check_arg_count(name, args, 1, 2)?;
            // Two-arg form: `iter(callable, sentinel)` calls `callable` with no
            // arguments on each `next()`, stopping when the result equals
            // `sentinel`. Lazy (CPython's `callable_iterator`) via a synthetic
            // generator, so `iter(int, 1)` and other unbounded forms stream a
            // value at a time rather than materialising — the sandbox's global
            // op/iteration limits bound any downstream consumption.
            if args.len() == 2 {
                let callable = args[0].clone();
                let sentinel = args[1].clone();
                if let Some(g) = lazy_gen_builtin(
                    state,
                    "<callable_iterator>",
                    "while True:\n    _v = _c()\n    if _v == _s:\n        return\n    yield _v\n",
                    &[("_c", callable.clone()), ("_s", sentinel.clone())],
                ) {
                    return Ok(Some(g));
                }
                // Fallback (body not suspend-drivable): eagerly materialise with
                // a hard cap so a non-terminating callable can't lock the loop.
                let mut out: Vec<Value> = Vec::new();
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
            make_iterator(state, &args[0], tools).await.map(Some)
        }
        "bytes" | "bytearray" => {
            // CPython: bytes() -> b''; bytes(int) -> b'\x00' * n;
            // bytes(iterable_of_ints) -> bytes from each int;
            // bytes(str, encoding) -> str.encode(encoding). `bytearray` yields a
            // mutable Value::ByteArray; `bytes` an immutable Value::Bytes.
            let raw: Vec<u8> = if args.is_empty() {
                Vec::new()
            } else {
                match &args[0] {
                    Value::Int(n) => {
                        // `bytes(-5)` raises ValueError; the old `.max(0)`
                        // silently produced an empty bytes.
                        let count = usize::try_from(*n).map_err(|_| {
                            EvalError::from(InterpreterError::ValueError("negative count".into()))
                        })?;
                        vec![0u8; count]
                    }
                    Value::Bytes(b) => b.clone(),
                    Value::ByteArray(b) => b.lock().clone(),
                    Value::MemoryView(_) => crate::types::memoryview_bytes(&args[0]),
                    Value::String(s) => {
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
                            "utf-8" | "utf_8" | "ascii" => s.as_bytes().to_vec(),
                            other => {
                                return Err(EvalError::Exception(
                                    crate::value::ExceptionValue::new(
                                        "LookupError",
                                        format!("unknown encoding: {other}"),
                                    ),
                                ));
                            }
                        }
                    }
                    // Any iterable of ints (list, tuple, range, set, generator).
                    other => {
                        let items = crate::eval::op::iter(state, other, tools).await?;
                        bytes_from_int_items(&items)?
                    }
                }
            };
            if name == "bytearray" {
                Ok(Some(Value::ByteArray(crate::value::shared_bytes(raw))))
            } else {
                Ok(Some(Value::Bytes(raw)))
            }
        }
        "next" => {
            check_arg_count(name, args, 1, 2)?;
            // Generator iterators: read the cursor, advance, return
            // the item at the old cursor; StopIteration when exhausted
            // (default arg returns it instead, matching CPython's
            // `next(g, sentinel)` shape).
            if let Value::Lazy { items, cursor_id, .. } = &args[0] {
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
            if matches!(&args[0], Value::Generator { .. } | Value::BuiltinIter { .. }) {
                match super::generators::dispatch_generator_method(
                    state,
                    &args[0],
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
            // Over an infinite producer, filter is lazy (CPython's `filter`
            // object) — `next(filter(pred, count()))` streams, not hangs.
            if has_infinite_arg(&args[1..]) {
                if let Some(g) = lazy_gen_builtin(
                    state,
                    "<filter>",
                    "for x in it:\n    if (x if pred is None else pred(x)):\n        yield x\n",
                    &[("pred", func.clone()), ("it", args[1].clone())],
                ) {
                    return Ok(Some(g));
                }
            }
            let iterable = crate::eval::op::iter(state, &args[1], tools).await?;
            let mut result = Vec::new();
            for item in iterable {
                // Async truthiness so a filtered instance (or an instance the
                // predicate returns) dispatches __bool__/__len__.
                let keep = if matches!(func, Value::None) {
                    crate::eval::op::truthy(state, &item, tools).await?
                } else {
                    let val = call_value_as_function(
                        state,
                        func,
                        std::slice::from_ref(&item),
                        &IndexMap::new(),
                        tools,
                    )
                    .await?;
                    crate::eval::op::truthy(state, &val, tools).await?
                };
                if keep {
                    result.push(item);
                }
            }
            Ok(Some(into_iter_value(state, result, LazyKind::Filter)))
        }
        "map" => {
            if args.len() < 2 {
                return Err(InterpreterError::TypeError(
                    "map() requires at least 2 arguments".into(),
                )
                .into());
            }
            let func = &args[0];
            // Over an infinite producer, map is lazy (CPython's `map` object).
            if has_infinite_arg(&args[1..]) {
                if let Some(g) = lazy_gen_builtin(
                    state,
                    "<map>",
                    "for row in zip(*iters):\n    yield func(*row)\n",
                    &[
                        ("func", func.clone()),
                        ("iters", Value::List(shared_list(args[1..].to_vec()))),
                    ],
                ) {
                    return Ok(Some(g));
                }
            }
            if args.len() == 2 {
                let iterable = crate::eval::op::iter(state, &args[1], tools).await?;
                let mut result = Vec::new();
                for item in iterable {
                    let val = call_value_as_function(state, func, &[item], &IndexMap::new(), tools)
                        .await?;
                    result.push(val);
                }
                Ok(Some(into_iter_value(state, result, LazyKind::Map)))
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
                Ok(Some(into_iter_value(state, result, LazyKind::Map)))
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
        "ascii" => {
            // Like repr(), but every non-ASCII code point is backslash-escaped.
            check_arg_count(name, args, 1, 1)?;
            let rendered = crate::eval::render::render(
                state,
                &args[0],
                crate::eval::render::RenderMode::Ascii,
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
        "dir" => {
            // Security posture (critically assessed): `dir` of a builtin VALUE
            // returns only universal, access-gated attribute-name strings — it
            // grants no access (`obj.__class__` still raises), reveals no sandbox
            // internals, and cannot escalate (getattr gates every name), so it is
            // safe and matches CPython. The no-arg form (== locals()) and dir of
            // instances / classes / modules / tools stay BLOCKED (scope / internals
            // leak), like `vars()`/`locals()`.
            check_arg_count(name, args, 0, 1)?;
            let Some(arg) = args.first() else {
                return Err(InterpreterError::TypeError(
                    "dir() with no arguments is not supported in the sandboxed interpreter (locals() is blocked)".into(),
                )
                .into());
            };
            let obj = resolve_proxy(arg).await?;
            let Some(names) = crate::types::builtin_dir(&obj) else {
                return Err(InterpreterError::TypeError(format!(
                    "dir() of a '{}' is not supported in the sandboxed interpreter (only builtin values)",
                    obj.type_name()
                ))
                .into());
            };
            let items = names.into_iter().map(|s| Value::String(s.into())).collect();
            Ok(Some(Value::List(shared_list(items))))
        }
        "__import__" => {
            // `__import__(name, globals=None, locals=None, fromlist=(), level=0)`.
            // The dynamic form of the `import` statement — routes through the same
            // module allow-list (`is_known_module`), so it inherits the sandbox's
            // import posture with no new surface. Only the flat top-level form is
            // supported (matching the `import` statement); dotted names and
            // relative imports (`level > 0`) are rejected.
            check_arg_count(name, args, 1, 5)?;
            let Value::String(module) = &args[0] else {
                return Err(InterpreterError::TypeError(format!(
                    "__import__() argument 1 must be str, not {}",
                    args[0].type_name()
                ))
                .into());
            };
            if let Some(level) = args.get(4) {
                let nonzero = match level {
                    Value::Int(n) => *n != 0,
                    Value::Bool(b) => *b,
                    _ => false,
                };
                if nonzero {
                    return Err(InterpreterError::Security(
                        "relative imports are not supported (see CONFORMANCE.md#import-allowlist)"
                            .into(),
                    )
                    .into());
                }
            }
            if module.contains('.') {
                return Err(InterpreterError::Security(
                    "dotted/submodule imports are not supported (see CONFORMANCE.md#import-allowlist)"
                        .into(),
                )
                .into());
            }
            if !crate::eval::modules::is_known_module(module) {
                return Err(crate::eval::modules::module_not_found(module));
            }
            Ok(Some(Value::Module(module.to_string())))
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
            // `bin`/`oct`/`hex` accept anything with `__index__`, coerced first.
            let n =
                value_to_i64(&crate::eval::op::coerce_index(state, args[0].clone(), tools).await?)?;
            let formatted =
                if n < 0 { format!("-0b{:b}", n.unsigned_abs()) } else { format!("0b{n:b}") };
            Ok(Some(Value::String(formatted.into())))
        }
        "oct" => {
            check_arg_count(name, args, 1, 1)?;
            let n =
                value_to_i64(&crate::eval::op::coerce_index(state, args[0].clone(), tools).await?)?;
            let formatted =
                if n < 0 { format!("-0o{:o}", n.unsigned_abs()) } else { format!("0o{n:o}") };
            Ok(Some(Value::String(formatted.into())))
        }
        "hex" => {
            check_arg_count(name, args, 1, 1)?;
            let n =
                value_to_i64(&crate::eval::op::coerce_index(state, args[0].clone(), tools).await?)?;
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

/// CPython 3.12's float fast path for `sum()`: Neumaier-compensated
/// summation of a numeric run. Returns `None` (defer to `__add__`) when
/// `start` or any item is non-numeric, or when nothing is a float (a
/// pure-int sum stays exact — and arbitrary-precision — on the generic
/// path).
/// Build a fresh iterator (`iter(value)` / `value.__iter__()`): an existing
/// iterator (or user object with `__next__`) is returned as-is; any other
/// iterable is materialised into a cursor-backed [`Value::Lazy`]. Shared by the
/// `iter` builtin and the `__iter__` dunder dispatch so both agree.
pub(crate) async fn make_iterator(
    state: &mut InterpreterState,
    value: &Value,
    tools: &Tools,
) -> Result<Value, EvalError> {
    match value {
        // Already an iterator — CPython returns it unchanged (iter(it) is it).
        Value::Lazy { .. } | Value::Generator { .. } | Value::BuiltinIter { .. } => {
            Ok(value.clone())
        }
        // A user object that already defines __next__ is its own iterator.
        Value::Instance(inst)
            if crate::eval::classes::lookup_method_in_mro(state, &inst.class_name, "__next__")
                .is_some() =>
        {
            Ok(value.clone())
        }
        // `iter(list)` shares the underlying list, so items appended (or
        // removed) before the cursor reaches them are observed — CPython's
        // `list_iterator` reference semantics, which an eager snapshot loses.
        Value::List(items) => Ok(state.alloc_builtin_iter(
            crate::value::BuiltinIterName::ListIterator,
            crate::state::BuiltinIterState::ListIter { list: items.clone(), index: 0 },
        )),
        Value::ByteArray(data) => Ok(state.alloc_builtin_iter(
            crate::value::BuiltinIterName::BytearrayIterator,
            crate::state::BuiltinIterState::BytearrayIter { data: data.clone(), index: 0 },
        )),
        // Immutable sources: a snapshot is behaviourally identical to a live
        // cursor, so materialise and tag with CPython's iterator type name.
        Value::Tuple(items) => Ok(state.alloc_lazy_kind(items.clone(), LazyKind::TupleIterator)),
        Value::String(s) => {
            // CPython uses `str_ascii_iterator` for an all-ASCII string and
            // `str_iterator` otherwise.
            let kind =
                if s.is_ascii() { LazyKind::StrAsciiIterator } else { LazyKind::StrIterator };
            let items = crate::eval::op::iter(state, value, tools).await?;
            Ok(state.alloc_lazy_kind(items, kind))
        }
        Value::Set(_) | Value::Frozenset(_) => {
            let items = crate::eval::op::iter(state, value, tools).await?;
            Ok(state.alloc_lazy_kind(items, LazyKind::SetIterator))
        }
        Value::Range { .. } => {
            let items = crate::eval::op::iter(state, value, tools).await?;
            Ok(state.alloc_lazy_kind(items, LazyKind::RangeIterator))
        }
        Value::Bytes(_) => {
            let items = crate::eval::op::iter(state, value, tools).await?;
            Ok(state.alloc_lazy_kind(items, LazyKind::BytesIterator))
        }
        // `iter(dict)` iterates keys; a dict view iterates keys/values/items.
        Value::Dict(_) | Value::OrderedDict(_) | Value::DefaultDict(_) | Value::Counter(_) => {
            let items = crate::eval::op::iter(state, value, tools).await?;
            Ok(state.alloc_lazy_kind(items, LazyKind::DictKeyIterator))
        }
        Value::DictView { kind, .. } => {
            let lazy_kind = match kind {
                crate::value::DictViewKind::Keys => LazyKind::DictKeyIterator,
                crate::value::DictViewKind::Values => LazyKind::DictValueIterator,
                crate::value::DictViewKind::Items => LazyKind::DictItemIterator,
            };
            let items = crate::eval::op::iter(state, value, tools).await?;
            Ok(state.alloc_lazy_kind(items, lazy_kind))
        }
        // Any other iterable: materialise into a fresh cursor-backed iterator.
        // `op::iter` raises TypeError for a non-iterable.
        other => {
            let items = crate::eval::op::iter(state, other, tools).await?;
            Ok(state.alloc_lazy(items))
        }
    }
}

fn sum_float_neumaier(start: &Value, items: &[Value]) -> Option<Value> {
    use num_traits::ToPrimitive as _;
    let as_num = |v: &Value| -> Option<(f64, bool)> {
        match v {
            #[allow(clippy::cast_precision_loss)]
            Value::Int(i) => Some((*i as f64, false)),
            Value::Bool(b) => Some((f64::from(u8::from(*b)), false)),
            Value::Float(f) => Some((*f, true)),
            Value::BigInt(b) => Some((b.to_f64()?, false)),
            _ => None,
        }
    };
    let (mut sum, mut any_float) = as_num(start)?;
    let mut nums = Vec::with_capacity(items.len());
    for it in items {
        let (f, is_float) = as_num(it)?;
        any_float |= is_float;
        nums.push(f);
    }
    if !any_float {
        return None;
    }
    // Neumaier (improved Kahan) compensated summation.
    let mut c = 0.0_f64;
    for x in nums {
        let t = sum + x;
        if sum.abs() >= x.abs() {
            c += (sum - t) + x;
        } else {
            c += (x - t) + sum;
        }
        sum = t;
    }
    Some(Value::Float(sum + c))
}

/// `zip(...)` when at least one argument is an infinite `BuiltinIter`.
/// Finite arguments are materialised; the lazy ones are pulled one item
/// per round. Iteration stops as soon as any argument is exhausted —
/// pulling left-to-right and discarding a partial round, matching
/// CPython. `zip(count(), count())` (all-infinite) never terminates,
/// True for an *infinite* builtin producer (`count`/`cycle`/`repeat`). A builtin
/// consumed over one of these must be lazy or it hangs materialising the source.
fn has_infinite_arg(args: &[Value]) -> bool {
    args.iter().any(|a| matches!(a, Value::BuiltinIter { .. }))
}

/// Build a lazy builtin (`zip`/`map`/`filter`/`enumerate`) as a synthesized
/// generator: parse a Python body template and bind its free parameters as
/// frame locals so the body's `for x in <input>` streams one item at a time.
/// Returns `None` if the body is not suspend-drivable.
fn lazy_gen_builtin(
    state: &mut InterpreterState,
    name: &str,
    body_src: &str,
    bindings: &[(&str, Value)],
) -> Option<Value> {
    let body = crate::parser::parse(body_src).ok()?;
    let (assigned, _globals) = crate::eval::functions::collect_assigned_names(&body);
    let mut locals: rustc_hash::FxHashMap<String, Value> = rustc_hash::FxHashMap::default();
    let mut touched: Vec<String> = Vec::new();
    for (n, v) in bindings {
        locals.insert((*n).to_string(), v.clone());
        touched.push((*n).to_string());
    }
    for n in assigned {
        if !touched.iter().any(|t| t == &n) {
            touched.push(n);
        }
    }
    crate::eval::functions::create_synthetic_generator(
        state,
        name,
        std::sync::Arc::new(body),
        locals,
        touched,
    )
}

/// Build a truly lazy `zip` as a synthesized generator that pulls one row at a
/// time and stops when any argument is exhausted — so `zip(count(), count())`
/// streams instead of hanging. Returns `None` if the body is not suspend-
/// drivable (caller falls back to `zip_lazy`).
fn zip_lazy_gen(state: &mut InterpreterState, args: &[Value]) -> Option<Value> {
    lazy_gen_builtin(
        state,
        "<zip>",
        "its = [iter(a) for a in sources]\n_missing = object()\ndone = False\nwhile not done:\n    row = []\n    for it in its:\n        v = next(it, _missing)\n        if v is _missing:\n            done = True\n            break\n        row.append(v)\n    if not done:\n        yield tuple(row)\n",
        &[("sources", Value::List(shared_list(args.to_vec())))],
    )
}

/// same as CPython.
async fn zip_lazy(
    state: &mut InterpreterState,
    args: &[Value],
    tools: &Tools,
) -> Result<Value, EvalError> {
    enum Src {
        Eager(Vec<Value>),
        Lazy(Value),
    }
    let mut srcs: Vec<Src> = Vec::with_capacity(args.len());
    for arg in args {
        if matches!(arg, Value::BuiltinIter { .. }) {
            srcs.push(Src::Lazy(arg.clone()));
        } else {
            srcs.push(Src::Eager(crate::eval::op::iter(state, arg, tools).await?));
        }
    }
    let empty = IndexMap::new();
    let mut out = Vec::new();
    let mut round = 0usize;
    'outer: loop {
        let mut tuple = Vec::with_capacity(srcs.len());
        for src in &srcs {
            let item = match src {
                Src::Eager(v) => match v.get(round) {
                    Some(item) => item.clone(),
                    None => break 'outer,
                },
                Src::Lazy(handle) => match super::generators::dispatch_generator_method(
                    state,
                    handle,
                    "__next__",
                    &[],
                    &empty,
                    tools,
                )
                .await
                {
                    Ok(item) => item,
                    Err(EvalError::Exception(e)) if e.type_name == "StopIteration" => break 'outer,
                    Err(e) => return Err(e),
                },
            };
            tuple.push(item);
        }
        out.push(Value::Tuple(tuple));
        round += 1;
    }
    Ok(into_iter_value(state, out, LazyKind::Zip))
}
