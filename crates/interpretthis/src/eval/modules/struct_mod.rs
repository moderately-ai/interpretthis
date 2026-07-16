// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `struct` module: `pack` / `unpack` / `calcsize`.
//!
//! Supports every byte-order prefix:
//!   - `<` / `>` / `!` / `=` — standard sizes, no alignment (portable formats);
//!   - `@` (and no prefix) — native byte order (little-endian on our targets),
//!     native sizes (`l`/`L` are 8 bytes), and native alignment padding.

use indexmap::IndexMap;
use num_traits::ToPrimitive as _;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    tools::Tools,
    value::Value,
};

pub struct StructModule;

#[async_trait::async_trait]
impl super::Module for StructModule {
    fn name(&self) -> &'static str {
        "struct"
    }

    fn constant(&self, name: &str) -> Option<Value> {
        // `struct.error` is the module's exception class, used in
        // `except struct.error`. It matches the raised exception's type name.
        (name == "error").then(|| Value::ExceptionType("struct.error".to_string()))
    }

    fn has_function(&self, name: &str) -> bool {
        matches!(name, "pack" | "unpack" | "calcsize")
    }

    async fn call(
        &self,
        _state: &mut InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &IndexMap<String, Value>,
        _tools: &Tools,
    ) -> EvalResult {
        match func {
            "calcsize" => {
                let (mode, items) = parse_format(str_arg(args.first(), "calcsize")?)?;
                Ok(Value::Int(i64::try_from(layout_size(mode, &items)).unwrap_or(0)))
            }
            "pack" => {
                let (mode, items) = parse_format(str_arg(args.first(), "pack")?)?;
                pack(mode, &items, &args[1..])
            }
            "unpack" => {
                let (mode, items) = parse_format(str_arg(args.first(), "unpack")?)?;
                unpack(mode, &items, &bytes_arg(args.get(1))?)
            }
            _ => Err(InterpreterError::AttributeError(format!(
                "module 'struct' has no callable '{func}'"
            ))
            .into()),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Little,
    Big,
    Native,
}

impl Mode {
    /// Big-endian byte layout in the packed stream?
    fn big_endian(self) -> bool {
        // Native byte order is little-endian on every platform we target.
        self == Self::Big
    }
    fn native(self) -> bool {
        self == Self::Native
    }
}

/// One parsed format field: the type code and its repeat count (for `s`, the
/// count is the byte width of a single string field).
struct Field {
    code: char,
    count: usize,
}

fn str_arg<'a>(v: Option<&'a Value>, func: &str) -> Result<&'a str, EvalError> {
    match v {
        Some(Value::String(s)) => Ok(s.as_str()),
        _ => Err(InterpreterError::TypeError(format!(
            "struct.{func}() argument 1 must be str, not {}",
            v.map_or("nothing", Value::type_name)
        ))
        .into()),
    }
}

fn bytes_arg(v: Option<&Value>) -> Result<Vec<u8>, EvalError> {
    match v {
        Some(Value::Bytes(b)) => Ok(b.clone()),
        Some(Value::ByteArray(b)) => Ok(b.lock().clone()),
        _ => {
            Err(InterpreterError::TypeError("unpack() requires a bytes-like object".into()).into())
        }
    }
}

fn struct_error(msg: impl Into<String>) -> EvalError {
    EvalError::Exception(crate::value::ExceptionValue::new("struct.error", msg.into()))
}

/// Byte width of one element of `code` in the given mode (`l`/`L` widen to 8 in
/// native mode). `x`/`s` are one byte per element/char.
fn code_size(code: char, mode: Mode) -> Result<usize, EvalError> {
    Ok(match code {
        'x' | 'c' | 'b' | 'B' | '?' | 's' => 1,
        'h' | 'H' => 2,
        'i' | 'I' | 'f' => 4,
        'l' | 'L' => {
            if mode.native() {
                8
            } else {
                4
            }
        }
        'q' | 'Q' | 'd' => 8,
        other => return Err(struct_error(format!("bad char in struct format: '{other}'"))),
    })
}

/// Native alignment of `code` (its size), or 1 in standard modes / for
/// non-aligned codes.
fn code_align(code: char, mode: Mode) -> usize {
    if mode.native() && !matches!(code, 'x' | 'c' | 's' | 'b' | 'B' | '?') {
        code_size(code, mode).unwrap_or(1)
    } else {
        1
    }
}

fn parse_format(fmt: &str) -> Result<(Mode, Vec<Field>), EvalError> {
    let mut chars = fmt.chars().peekable();
    let mode = match chars.peek() {
        Some('<') => {
            chars.next();
            Mode::Little
        }
        Some('>' | '!') => {
            chars.next();
            Mode::Big
        }
        Some('=') => {
            chars.next();
            Mode::Little
        }
        Some('@') => {
            chars.next();
            Mode::Native
        }
        _ => Mode::Native,
    };
    let mut fields = Vec::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_whitespace() {
            chars.next();
            continue;
        }
        if c.is_ascii_digit() {
            let mut n = 0usize;
            while let Some(&d) = chars.peek() {
                if let Some(dig) = d.to_digit(10) {
                    n = n * 10 + dig as usize;
                    chars.next();
                } else {
                    break;
                }
            }
            let code = chars
                .next()
                .ok_or_else(|| struct_error("repeat count given without format specifier"))?;
            code_size(code, mode)?;
            fields.push(Field { code, count: n });
        } else {
            code_size(c, mode)?;
            chars.next();
            fields.push(Field { code: c, count: 1 });
        }
    }
    Ok((mode, fields))
}

fn align_to(offset: usize, alignment: usize) -> usize {
    if alignment > 1 { offset.div_ceil(alignment) * alignment } else { offset }
}

/// Total packed size including native alignment padding.
fn layout_size(mode: Mode, fields: &[Field]) -> usize {
    let mut offset = 0usize;
    for f in fields {
        offset = align_to(offset, code_align(f.code, mode));
        offset +=
            if f.code == 's' { f.count } else { f.count * code_size(f.code, mode).unwrap_or(0) };
    }
    offset
}

/// Number of Python values a format consumes/produces (`x` none, `s` one).
fn value_count(fields: &[Field]) -> usize {
    fields
        .iter()
        .map(|f| match f.code {
            'x' => 0,
            's' => 1,
            _ => f.count,
        })
        .sum()
}

fn as_i128(v: &Value) -> Result<i128, EvalError> {
    match v {
        Value::Int(i) => Ok(i128::from(*i)),
        Value::Bool(b) => Ok(i128::from(*b)),
        Value::BigInt(b) => b.to_i128().ok_or_else(|| struct_error("argument out of range")),
        _ => Err(struct_error("required argument is not an integer")),
    }
}

fn as_f64(v: &Value) -> Result<f64, EvalError> {
    match v {
        Value::Float(f) => Ok(*f),
        Value::Int(i) => Ok(*i as f64),
        Value::Bool(b) => Ok(f64::from(*b)),
        _ => Err(struct_error("required argument is not a float")),
    }
}

/// Emit `bytes` (given big-endian) into `out` honouring the stream's byte order.
fn put(out: &mut Vec<u8>, mode: Mode, bytes: &[u8]) {
    if mode.big_endian() {
        out.extend_from_slice(bytes);
    } else {
        out.extend(bytes.iter().rev().copied());
    }
}

fn range_err(code: char) -> EvalError {
    let msg = match code {
        'b' => "byte format requires -128 <= number <= 127",
        'B' => "ubyte format requires 0 <= number <= 255",
        'h' => "short format requires -32768 <= number <= 32767",
        'H' => "ushort format requires 0 <= number <= 65535",
        _ => "argument out of range",
    };
    struct_error(msg)
}

fn pack(mode: Mode, fields: &[Field], values: &[Value]) -> EvalResult {
    let expected = value_count(fields);
    if values.len() != expected {
        return Err(struct_error(format!(
            "pack expected {expected} items for packing (got {})",
            values.len()
        )));
    }
    let mut out = Vec::new();
    let mut vi = 0;
    for f in fields {
        // Native alignment padding.
        let pad = align_to(out.len(), code_align(f.code, mode)) - out.len();
        out.extend(std::iter::repeat_n(0u8, pad));
        match f.code {
            'x' => out.extend(std::iter::repeat_n(0u8, f.count)),
            's' => {
                let mut field = match &values[vi] {
                    Value::Bytes(b) => b.clone(),
                    Value::ByteArray(b) => b.lock().clone(),
                    _ => return Err(struct_error("argument for 's' must be a bytes object")),
                };
                field.resize(f.count, 0);
                out.extend_from_slice(&field);
                vi += 1;
            }
            _ => {
                let native_long = mode.native() && matches!(f.code, 'l' | 'L');
                for _ in 0..f.count {
                    let v = &values[vi];
                    vi += 1;
                    match f.code {
                        'c' => match v {
                            Value::Bytes(b) if b.len() == 1 => out.push(b[0]),
                            _ => {
                                return Err(struct_error(
                                    "char format requires a bytes object of length 1",
                                ));
                            }
                        },
                        '?' => {
                            out.push(u8::from(crate::eval::op::try_truthy_sync(v).unwrap_or(true)))
                        }
                        'b' => put(
                            &mut out,
                            mode,
                            &[i8::try_from(as_i128(v)?).map_err(|_| range_err('b'))? as u8],
                        ),
                        'B' => put(
                            &mut out,
                            mode,
                            &[u8::try_from(as_i128(v)?).map_err(|_| range_err('B'))?],
                        ),
                        'h' => put(
                            &mut out,
                            mode,
                            &i16::try_from(as_i128(v)?).map_err(|_| range_err('h'))?.to_be_bytes(),
                        ),
                        'H' => put(
                            &mut out,
                            mode,
                            &u16::try_from(as_i128(v)?).map_err(|_| range_err('H'))?.to_be_bytes(),
                        ),
                        'i' => put(
                            &mut out,
                            mode,
                            &i32::try_from(as_i128(v)?).map_err(|_| range_err('i'))?.to_be_bytes(),
                        ),
                        'I' => put(
                            &mut out,
                            mode,
                            &u32::try_from(as_i128(v)?).map_err(|_| range_err('I'))?.to_be_bytes(),
                        ),
                        'l' if !native_long => put(
                            &mut out,
                            mode,
                            &i32::try_from(as_i128(v)?).map_err(|_| range_err('l'))?.to_be_bytes(),
                        ),
                        'L' if !native_long => put(
                            &mut out,
                            mode,
                            &u32::try_from(as_i128(v)?).map_err(|_| range_err('L'))?.to_be_bytes(),
                        ),
                        'q' | 'l' => put(
                            &mut out,
                            mode,
                            &i64::try_from(as_i128(v)?).map_err(|_| range_err('q'))?.to_be_bytes(),
                        ),
                        'Q' | 'L' => put(
                            &mut out,
                            mode,
                            &u64::try_from(as_i128(v)?).map_err(|_| range_err('Q'))?.to_be_bytes(),
                        ),
                        'f' => put(&mut out, mode, &(as_f64(v)? as f32).to_be_bytes()),
                        'd' => put(&mut out, mode, &as_f64(v)?.to_be_bytes()),
                        _ => unreachable!("validated in parse_format"),
                    }
                }
            }
        }
    }
    Ok(Value::Bytes(out))
}

/// Copy the first 8 bytes of an (always-8-byte) slice into a fixed array,
/// avoiding a fallible `try_into` on the `deny(unwrap_used)` build.
fn eight(b: &[u8]) -> [u8; 8] {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[..8]);
    a
}

/// Read `n` big-endian bytes from `buf` at `pos`, advancing it.
fn take(buf: &[u8], pos: &mut usize, n: usize, mode: Mode) -> Result<Vec<u8>, EvalError> {
    if *pos + n > buf.len() {
        return Err(struct_error("unpack requires a buffer of the declared size"));
    }
    let mut slice = buf[*pos..*pos + n].to_vec();
    *pos += n;
    if !mode.big_endian() {
        slice.reverse();
    }
    Ok(slice)
}

fn unpack(mode: Mode, fields: &[Field], buf: &[u8]) -> EvalResult {
    let need = layout_size(mode, fields);
    if buf.len() != need {
        return Err(struct_error(format!("unpack requires a buffer of {need} bytes")));
    }
    let mut pos = 0usize;
    let mut out: Vec<Value> = Vec::new();
    for f in fields {
        pos = align_to(pos, code_align(f.code, mode));
        match f.code {
            'x' => pos += f.count,
            's' => {
                out.push(Value::Bytes(buf[pos..pos + f.count].to_vec()));
                pos += f.count;
            }
            _ => {
                let sz = code_size(f.code, mode)?;
                let native_long = mode.native() && matches!(f.code, 'l' | 'L');
                for _ in 0..f.count {
                    let b = take(buf, &mut pos, sz, mode)?;
                    let v = match f.code {
                        'c' => Value::Bytes(vec![b[0]]),
                        '?' => Value::Bool(b[0] != 0),
                        'b' => Value::Int(i64::from(i8::from_be_bytes([b[0]]))),
                        'B' => Value::Int(i64::from(b[0])),
                        'h' => Value::Int(i64::from(i16::from_be_bytes([b[0], b[1]]))),
                        'H' => Value::Int(i64::from(u16::from_be_bytes([b[0], b[1]]))),
                        'i' => Value::Int(i64::from(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))),
                        'I' => Value::Int(i64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))),
                        'l' if !native_long => {
                            Value::Int(i64::from(i32::from_be_bytes([b[0], b[1], b[2], b[3]])))
                        }
                        'L' if !native_long => {
                            Value::Int(i64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]])))
                        }
                        'q' | 'l' => Value::Int(i64::from_be_bytes(eight(&b))),
                        'Q' | 'L' => {
                            let u = u64::from_be_bytes(eight(&b));
                            i64::try_from(u).map_or_else(
                                |_| crate::value::int_from_bigint(u.into()),
                                Value::Int,
                            )
                        }
                        'f' => {
                            Value::Float(f64::from(f32::from_be_bytes([b[0], b[1], b[2], b[3]])))
                        }
                        'd' => Value::Float(f64::from_be_bytes(eight(&b))),
                        _ => unreachable!("validated in parse_format"),
                    };
                    out.push(v);
                }
            }
        }
    }
    Ok(Value::Tuple(out))
}
