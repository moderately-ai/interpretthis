// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `datetime` module — date / datetime / time /
//! timedelta / timezone (Track D).
//!
//! Storage maps to chrono types: `NaiveDate` for `date`,
//! `NaiveDateTime` for `datetime` (with an optional fixed UTC offset),
//! `NaiveTime` for `time`, raw microseconds for `timedelta`, raw
//! seconds for `timezone`. Arithmetic between date / datetime and
//! timedelta lands here. Aware vs naive `datetime` mixing raises
//! TypeError per CPython.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::modules::{arg_str, need_arg, value_error},
    value::Value,
};

/// Whether `datetime` provides a function named `name`.
pub fn has_function(name: &str) -> bool {
    matches!(name, "date" | "datetime" | "time" | "timedelta" | "timezone")
}

/// Build a `NaiveDate` with CPython's exact validation order +
/// wording: year first (`year N is out of range`), then month
/// (`month must be in 1..12`), then day (`day is out of range for
/// month`). chrono's `from_ymd_opt` returns `None` for any
/// invalidity without saying which — so we pre-validate each
/// component to attach the right CPython message.
fn construct_naive_date(year: i32, month: u32, day: u32) -> Result<NaiveDate, EvalError> {
    if !(1..=9999).contains(&year) {
        return Err(value_error(format!("year {year} is out of range")));
    }
    if !(1..=12).contains(&month) {
        return Err(value_error("month must be in 1..12"));
    }
    NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| value_error("day is out of range for month"))
}

/// Invoke a `datetime` constructor.
pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        "date" => {
            let year = arg_i32(func, args, 0)?;
            let month = arg_u32(func, args, 1)?;
            let day = arg_u32(func, args, 2)?;
            let date = construct_naive_date(year, month, day)?;
            Ok(Value::Date(date))
        }
        "datetime" => {
            // datetime(year, month, day, hour=0, minute=0, second=0,
            //          microsecond=0, tzinfo=None) — positional only
            // here because the method-dispatch path doesn't thread
            // kwargs. Tzinfo arg is accepted as a Value::TimeZone or
            // None.
            let year = arg_i32(func, args, 0)?;
            let month = arg_u32(func, args, 1)?;
            let day = arg_u32(func, args, 2)?;
            let hour = opt_u32(args, 3)?.unwrap_or(0);
            let minute = opt_u32(args, 4)?.unwrap_or(0);
            let second = opt_u32(args, 5)?.unwrap_or(0);
            let microsecond = opt_u32(args, 6)?.unwrap_or(0);
            let date = construct_naive_date(year, month, day)?;
            let time = NaiveTime::from_hms_micro_opt(hour, minute, second, microsecond)
                .ok_or_else(|| value_error("time component out of range"))?;
            let dt = NaiveDateTime::new(date, time);
            let tz_offset_secs = match args.get(7) {
                None | Some(Value::None) => None,
                Some(Value::TimeZone(secs)) => Some(*secs),
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "datetime() tzinfo must be a datetime.timezone (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            Ok(Value::DateTime { dt, tz_offset_secs })
        }
        "time" => {
            // time(hour=0, minute=0, second=0, microsecond=0)
            let hour = opt_u32(args, 0)?.unwrap_or(0);
            let minute = opt_u32(args, 1)?.unwrap_or(0);
            let second = opt_u32(args, 2)?.unwrap_or(0);
            let microsecond = opt_u32(args, 3)?.unwrap_or(0);
            let t = NaiveTime::from_hms_micro_opt(hour, minute, second, microsecond)
                .ok_or_else(|| value_error("time component out of range"))?;
            Ok(Value::Time(t))
        }
        "timedelta" => {
            // timedelta(days=0, seconds=0, microseconds=0,
            //           milliseconds=0, minutes=0, hours=0, weeks=0)
            // Positional only; method dispatch doesn't carry kwargs.
            let days = opt_i64(args, 0)?.unwrap_or(0);
            let seconds = opt_i64(args, 1)?.unwrap_or(0);
            let microseconds = opt_i64(args, 2)?.unwrap_or(0);
            let milliseconds = opt_i64(args, 3)?.unwrap_or(0);
            let minutes = opt_i64(args, 4)?.unwrap_or(0);
            let hours = opt_i64(args, 5)?.unwrap_or(0);
            let weeks = opt_i64(args, 6)?.unwrap_or(0);
            // Compose into microseconds carefully — every component
            // converts via i64 multiplication, saturating-checked at
            // each step so 1e18-microsecond inputs surface as an
            // OverflowError rather than wrap silently.
            let micros = days
                .checked_mul(86_400_000_000)
                .and_then(|d| weeks.checked_mul(7 * 86_400_000_000).and_then(|w| d.checked_add(w)))
                .and_then(|x| hours.checked_mul(3_600_000_000).and_then(|h| x.checked_add(h)))
                .and_then(|x| minutes.checked_mul(60_000_000).and_then(|m| x.checked_add(m)))
                .and_then(|x| seconds.checked_mul(1_000_000).and_then(|s| x.checked_add(s)))
                .and_then(|x| milliseconds.checked_mul(1_000).and_then(|ms| x.checked_add(ms)))
                .and_then(|x| x.checked_add(microseconds))
                .ok_or_else(|| value_error("timedelta overflow"))?;
            Ok(Value::TimeDelta(micros))
        }
        "timezone" => {
            // timezone(offset) where offset is a timedelta.
            let offset = need_arg(func, args, 0)?;
            let secs = match offset {
                Value::TimeDelta(micros) => {
                    let secs_i64 = micros / 1_000_000;
                    i32::try_from(secs_i64)
                        .map_err(|_| value_error("timezone offset out of range"))?
                }
                _ => {
                    return Err(InterpreterError::TypeError(format!(
                        "timezone() argument 1 must be a timedelta (got '{}')",
                        offset.type_name()
                    ))
                    .into());
                }
            };
            Ok(Value::TimeZone(secs))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'datetime' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// Read an attribute of a `date` value (`.year`, `.month`, `.day`).
pub fn date_attribute(date: NaiveDate, attr: &str) -> EvalResult {
    match attr {
        "year" => Ok(Value::Int(i64::from(date.year()))),
        "month" => Ok(Value::Int(i64::from(date.month()))),
        "day" => Ok(Value::Int(i64::from(date.day()))),
        _ => Err(InterpreterError::AttributeError(format!(
            "'date' object has no attribute '{attr}'"
        ))
        .into()),
    }
}

/// Read an attribute of a `datetime` value.
pub fn datetime_attribute(
    dt: NaiveDateTime,
    tz_offset_secs: Option<i32>,
    attr: &str,
) -> EvalResult {
    match attr {
        "year" => Ok(Value::Int(i64::from(dt.year()))),
        "month" => Ok(Value::Int(i64::from(dt.month()))),
        "day" => Ok(Value::Int(i64::from(dt.day()))),
        "hour" => Ok(Value::Int(i64::from(dt.hour()))),
        "minute" => Ok(Value::Int(i64::from(dt.minute()))),
        "second" => Ok(Value::Int(i64::from(dt.second()))),
        "microsecond" => Ok(Value::Int(i64::from(dt.nanosecond() / 1_000))),
        "tzinfo" => Ok(tz_offset_secs.map_or(Value::None, Value::TimeZone)),
        _ => Err(InterpreterError::AttributeError(format!(
            "'datetime' object has no attribute '{attr}'"
        ))
        .into()),
    }
}

/// Read an attribute of a `time` value.
pub fn time_attribute(t: NaiveTime, attr: &str) -> EvalResult {
    match attr {
        "hour" => Ok(Value::Int(i64::from(t.hour()))),
        "minute" => Ok(Value::Int(i64::from(t.minute()))),
        "second" => Ok(Value::Int(i64::from(t.second()))),
        "microsecond" => Ok(Value::Int(i64::from(t.nanosecond() / 1_000))),
        _ => Err(InterpreterError::AttributeError(format!(
            "'time' object has no attribute '{attr}'"
        ))
        .into()),
    }
}

/// Read an attribute of a `timedelta` value.
pub fn timedelta_attribute(micros: i64, attr: &str) -> EvalResult {
    // CPython's timedelta normalises so seconds and microseconds are
    // non-negative; we match by using Euclidean division.
    let secs_total = micros.div_euclid(1_000_000);
    let us = micros.rem_euclid(1_000_000);
    let days = secs_total.div_euclid(86_400);
    let secs = secs_total.rem_euclid(86_400);
    match attr {
        "days" => Ok(Value::Int(days)),
        "seconds" => Ok(Value::Int(secs)),
        "microseconds" => Ok(Value::Int(us)),
        _ => Err(InterpreterError::AttributeError(format!(
            "'timedelta' object has no attribute '{attr}'"
        ))
        .into()),
    }
}

/// Dispatch a method on a `date` value.
pub fn dispatch_date_method(
    date: NaiveDate,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "isoformat" => Ok(Value::String(date.format("%Y-%m-%d").to_string().into())),
        // Python: Monday == 0 … Sunday == 6.
        "weekday" => Ok(Value::Int(i64::from(date.weekday().num_days_from_monday()))),
        // Python: Monday == 1 … Sunday == 7.
        "isoweekday" => Ok(Value::Int(i64::from(date.weekday().number_from_monday()))),
        "strftime" => {
            let fmt = arg_str("strftime", args, 0)?;
            Ok(Value::String(date.format(fmt).to_string().into()))
        }
        "replace" => {
            // Keyword args are not threaded through method dispatch, so only the
            // positional `replace(year, month, day)` form is supported.
            let year = opt_i32(args, 0)?.unwrap_or_else(|| date.year());
            let month = opt_u32(args, 1)?.unwrap_or_else(|| date.month());
            let day = opt_u32(args, 2)?.unwrap_or_else(|| date.day());
            let replaced = construct_naive_date(year, month, day)?;
            Ok(Value::Date(replaced))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'date' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Dispatch a method on a `datetime` value.
pub fn dispatch_datetime_method(
    dt: NaiveDateTime,
    tz_offset_secs: Option<i32>,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "isoformat" => {
            // CPython: `2026-01-15T14:30:00` for naive; with tz adds
            // `+HH:MM`. We don't model microseconds in the default
            // isoformat (CPython only shows them when nonzero).
            let mut s = dt.format("%Y-%m-%dT%H:%M:%S").to_string();
            let us = dt.nanosecond() / 1_000;
            if us != 0 {
                s = format!("{s}.{us:06}");
            }
            if let Some(secs) = tz_offset_secs {
                let sign = if secs < 0 { '-' } else { '+' };
                let abs = secs.unsigned_abs();
                let h = abs / 3600;
                let m = (abs % 3600) / 60;
                s = format!("{s}{sign}{h:02}:{m:02}");
            }
            Ok(Value::String(s.into()))
        }
        "date" => Ok(Value::Date(dt.date())),
        "time" => Ok(Value::Time(dt.time())),
        "weekday" => Ok(Value::Int(i64::from(dt.weekday().num_days_from_monday()))),
        "isoweekday" => Ok(Value::Int(i64::from(dt.weekday().number_from_monday()))),
        "strftime" => {
            let fmt = arg_str("strftime", args, 0)?;
            Ok(Value::String(dt.format(fmt).to_string().into()))
        }
        "timestamp" => {
            // CPython treats naive datetime as local time for
            // timestamp(); we treat naive as UTC for determinism (no
            // local-timezone access in the sandbox). Aware datetimes
            // subtract the offset.
            #[expect(
                clippy::cast_precision_loss,
                reason = "matches CPython's timestamp() return shape: f64 seconds since epoch. Loss matters only past 2^53 seconds (~285M years post-epoch)."
            )]
            let mut ts = dt.and_utc().timestamp_millis() as f64 / 1000.0;
            if let Some(secs) = tz_offset_secs {
                ts -= f64::from(secs);
            }
            Ok(Value::Float(ts))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'datetime' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Dispatch a method on a `time` value.
pub fn dispatch_time_method(
    t: NaiveTime,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "isoformat" => {
            let mut s = t.format("%H:%M:%S").to_string();
            let us = t.nanosecond() / 1_000;
            if us != 0 {
                s = format!("{s}.{us:06}");
            }
            Ok(Value::String(s.into()))
        }
        "strftime" => {
            let fmt = arg_str("strftime", args, 0)?;
            Ok(Value::String(t.format(fmt).to_string().into()))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'time' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Dispatch a method on a `timedelta` value.
pub fn dispatch_timedelta_method(
    micros: i64,
    method: &str,
    _args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    crate::eval::functions::reject_kwargs(method, kwargs)?;
    match method {
        "total_seconds" => {
            #[expect(
                clippy::cast_precision_loss,
                reason = "matches CPython's total_seconds() return shape: f64 seconds. Loss matters only past 2^53 microseconds (~285 years)."
            )]
            let secs = micros as f64 / 1_000_000.0;
            Ok(Value::Float(secs))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "'timedelta' object has no attribute '{method}'"
        ))
        .into()),
    }
}

/// Arithmetic between Date / DateTime / TimeDelta. Returns `None` when
/// the operands don't form a supported pair so the dispatcher can try
/// the reflected slot or raise TypeError.
pub fn try_arith(op: &str, lhs: &Value, rhs: &Value) -> Option<EvalResult> {
    match (op, lhs, rhs) {
        // date + timedelta -> date
        ("+", Value::Date(d), Value::TimeDelta(us))
        | ("+", Value::TimeDelta(us), Value::Date(d)) => {
            let days = i32::try_from(us / 86_400_000_000).ok()?;
            let result = d.checked_add_signed(Duration::days(i64::from(days)))?;
            Some(Ok(Value::Date(result)))
        }
        // date - timedelta -> date
        ("-", Value::Date(d), Value::TimeDelta(us)) => {
            let days = i32::try_from(us / 86_400_000_000).ok()?;
            let result = d.checked_sub_signed(Duration::days(i64::from(days)))?;
            Some(Ok(Value::Date(result)))
        }
        // date - date -> timedelta (days)
        ("-", Value::Date(a), Value::Date(b)) => {
            let delta = a.signed_duration_since(*b);
            Some(Ok(Value::TimeDelta(delta.num_microseconds()?)))
        }
        // datetime + timedelta -> datetime
        ("+", Value::DateTime { dt, tz_offset_secs }, Value::TimeDelta(us))
        | ("+", Value::TimeDelta(us), Value::DateTime { dt, tz_offset_secs }) => {
            let result = dt.checked_add_signed(Duration::microseconds(*us))?;
            Some(Ok(Value::DateTime { dt: result, tz_offset_secs: *tz_offset_secs }))
        }
        ("-", Value::DateTime { dt, tz_offset_secs }, Value::TimeDelta(us)) => {
            let result = dt.checked_sub_signed(Duration::microseconds(*us))?;
            Some(Ok(Value::DateTime { dt: result, tz_offset_secs: *tz_offset_secs }))
        }
        // datetime - datetime -> timedelta. CPython raises TypeError on
        // mixed aware/naive subtraction.
        (
            "-",
            Value::DateTime { dt: a, tz_offset_secs: tz_a },
            Value::DateTime { dt: b, tz_offset_secs: tz_b },
        ) => {
            if tz_a.is_some() != tz_b.is_some() {
                return Some(Err(InterpreterError::TypeError(
                    "can't subtract offset-naive and offset-aware datetimes".into(),
                )
                .into()));
            }
            let delta = a.signed_duration_since(*b);
            Some(Ok(Value::TimeDelta(delta.num_microseconds()?)))
        }
        // timedelta + timedelta
        ("+", Value::TimeDelta(a), Value::TimeDelta(b)) => {
            Some(Ok(Value::TimeDelta(a.checked_add(*b)?)))
        }
        ("-", Value::TimeDelta(a), Value::TimeDelta(b)) => {
            Some(Ok(Value::TimeDelta(a.checked_sub(*b)?)))
        }
        // timedelta * int -> timedelta (and reflected)
        ("*", Value::TimeDelta(a), Value::Int(n)) | ("*", Value::Int(n), Value::TimeDelta(a)) => {
            Some(Ok(Value::TimeDelta(a.checked_mul(*n)?)))
        }
        // timedelta / int -> timedelta (integer microseconds)
        ("//", Value::TimeDelta(a), Value::Int(n)) if *n != 0 => Some(Ok(Value::TimeDelta(a / n))),
        _ => None,
    }
}

fn arg_i32(func: &str, args: &[Value], index: usize) -> Result<i32, EvalError> {
    int_arg(need_arg(func, args, index)?, func, index)
        .and_then(|n| i32::try_from(n).map_err(|_| value_error("year out of range")))
}

fn arg_u32(func: &str, args: &[Value], index: usize) -> Result<u32, EvalError> {
    int_arg(need_arg(func, args, index)?, func, index)
        .and_then(|n| u32::try_from(n).map_err(|_| value_error("value out of range")))
}

fn opt_i32(args: &[Value], index: usize) -> Result<Option<i32>, EvalError> {
    match args.get(index) {
        None => Ok(None),
        Some(v) => Ok(Some(
            int_arg(v, "replace", index)
                .and_then(|n| i32::try_from(n).map_err(|_| value_error("year out of range")))?,
        )),
    }
}

fn opt_u32(args: &[Value], index: usize) -> Result<Option<u32>, EvalError> {
    match args.get(index) {
        None => Ok(None),
        Some(v) => Ok(Some(
            int_arg(v, "replace", index)
                .and_then(|n| u32::try_from(n).map_err(|_| value_error("value out of range")))?,
        )),
    }
}

fn opt_i64(args: &[Value], index: usize) -> Result<Option<i64>, EvalError> {
    match args.get(index) {
        None | Some(Value::None) => Ok(None),
        Some(v) => int_arg(v, "timedelta", index).map(Some),
    }
}

fn int_arg(value: &Value, func: &str, index: usize) -> Result<i64, EvalError> {
    match value {
        Value::Int(i) => Ok(*i),
        Value::Bool(b) => Ok(i64::from(*b)),
        _ => Err(InterpreterError::TypeError(format!(
            "{func}() expected an integer at position {index}"
        ))
        .into()),
    }
}

/// `datetime` module registration.
pub struct DatetimeModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for DatetimeModule {
    fn name(&self) -> &'static str {
        "datetime"
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
