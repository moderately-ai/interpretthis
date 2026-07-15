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
    eval::modules::{arg_str, value_error},
    value::Value,
};

/// Whether `datetime` provides a **module-level** callable named `name`.
/// Classmethods on constructors (e.g. `datetime.strptime`) are not listed
/// here — they resolve through [`type_classmethod`].
pub fn has_function(name: &str) -> bool {
    matches!(name, "date" | "datetime" | "time" | "timedelta" | "timezone")
}

/// Classmethods on datetime module constructors (`datetime.datetime.strptime`).
/// Returns the internal `call` function name when `type_name.method` is valid.
#[must_use]
pub fn type_classmethod(type_name: &str, method: &str) -> Option<&'static str> {
    match (type_name, method) {
        ("datetime", "strptime") => Some("strptime"),
        ("date", "fromisoformat") => Some("date.fromisoformat"),
        ("datetime", "fromisoformat") => Some("datetime.fromisoformat"),
        _ => None,
    }
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

/// Invoke a `datetime` constructor. Positional and keyword arguments are bound
/// through `bind_method_params`, so both `datetime(2020, 1, 1, hour=9)` and the
/// fully-positional form work and unknown/duplicate keywords raise TypeError.
pub fn call(func: &str, args: &[Value], kwargs: &indexmap::IndexMap<String, Value>) -> EvalResult {
    use crate::eval::functions::bind_method_params;
    match func {
        "date" => {
            let b = bind_method_params(func, args, kwargs, &["year", "month", "day"])?;
            let year = comp_i32(b[0].as_ref(), func, "year")?;
            let month = comp_u32(b[1].as_ref(), func, "month")?;
            let day = comp_u32(b[2].as_ref(), func, "day")?;
            let date = construct_naive_date(year, month, day)?;
            Ok(Value::Date(date))
        }
        "datetime" => {
            // datetime(year, month, day, hour=0, minute=0, second=0,
            //          microsecond=0, tzinfo=None, *, fold=0). Tzinfo is a
            // Value::TimeZone or None; fold is accepted but not modelled.
            let b = bind_method_params(
                func,
                args,
                kwargs,
                &[
                    "year",
                    "month",
                    "day",
                    "hour",
                    "minute",
                    "second",
                    "microsecond",
                    "tzinfo",
                    "fold",
                ],
            )?;
            let year = comp_i32(b[0].as_ref(), func, "year")?;
            let month = comp_u32(b[1].as_ref(), func, "month")?;
            let day = comp_u32(b[2].as_ref(), func, "day")?;
            let hour = comp_opt_u32(b[3].as_ref(), func)?.unwrap_or(0);
            let minute = comp_opt_u32(b[4].as_ref(), func)?.unwrap_or(0);
            let second = comp_opt_u32(b[5].as_ref(), func)?.unwrap_or(0);
            let microsecond = comp_opt_u32(b[6].as_ref(), func)?.unwrap_or(0);
            let date = construct_naive_date(year, month, day)?;
            let time = NaiveTime::from_hms_micro_opt(hour, minute, second, microsecond)
                .ok_or_else(|| value_error("time component out of range"))?;
            let dt = NaiveDateTime::new(date, time);
            let tz_offset_secs = match b[7].as_ref() {
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
            // time(hour=0, minute=0, second=0, microsecond=0, tzinfo=None,
            //      *, fold=0). Our time is naive; tzinfo/fold are accepted for
            // signature parity but not modelled.
            let b = bind_method_params(
                func,
                args,
                kwargs,
                &["hour", "minute", "second", "microsecond", "tzinfo", "fold"],
            )?;
            let hour = comp_opt_u32(b[0].as_ref(), func)?.unwrap_or(0);
            let minute = comp_opt_u32(b[1].as_ref(), func)?.unwrap_or(0);
            let second = comp_opt_u32(b[2].as_ref(), func)?.unwrap_or(0);
            let microsecond = comp_opt_u32(b[3].as_ref(), func)?.unwrap_or(0);
            let t = NaiveTime::from_hms_micro_opt(hour, minute, second, microsecond)
                .ok_or_else(|| value_error("time component out of range"))?;
            Ok(Value::Time(t))
        }
        "timedelta" => {
            let b = bind_method_params(
                func,
                args,
                kwargs,
                &["days", "seconds", "microseconds", "milliseconds", "minutes", "hours", "weeks"],
            )?;
            let days = comp_opt_i64(b[0].as_ref())?.unwrap_or(0);
            let seconds = comp_opt_i64(b[1].as_ref())?.unwrap_or(0);
            let microseconds = comp_opt_i64(b[2].as_ref())?.unwrap_or(0);
            let milliseconds = comp_opt_i64(b[3].as_ref())?.unwrap_or(0);
            let minutes = comp_opt_i64(b[4].as_ref())?.unwrap_or(0);
            let hours = comp_opt_i64(b[5].as_ref())?.unwrap_or(0);
            let weeks = comp_opt_i64(b[6].as_ref())?.unwrap_or(0);
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
            // timezone(offset, name=None) where offset is a timedelta.
            let b = bind_method_params(func, args, kwargs, &["offset", "name"])?;
            let offset = b[0]
                .as_ref()
                .ok_or_else(|| value_error("timezone() missing required argument 'offset'"))?;
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
        // Invoked only via type_classmethod resolution (not module_member).
        "strptime" => {
            let s = arg_str("strptime", args, 0)?;
            let fmt = arg_str("strptime", args, 1)?;
            parse_strptime(s, fmt)
        }
        "date.fromisoformat" => {
            let s = arg_str("fromisoformat", args, 0)?;
            NaiveDate::parse_from_str(s, "%Y-%m-%d").map(Value::Date).map_err(|_| {
                EvalError::from(InterpreterError::ValueError(format!(
                    "Invalid isoformat string: '{s}'"
                )))
            })
        }
        "datetime.fromisoformat" => {
            let s = arg_str("fromisoformat", args, 0)?;
            // Accept the common "T"/" "-separated forms, with or without seconds.
            let parsed = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
                .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
                .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
                .or_else(|_| {
                    NaiveDate::parse_from_str(s, "%Y-%m-%d")
                        .map(|d| d.and_hms_opt(0, 0, 0).unwrap_or_default())
                });
            parsed.map(|dt| Value::DateTime { dt, tz_offset_secs: None }).map_err(|_| {
                EvalError::from(InterpreterError::ValueError(format!(
                    "Invalid isoformat string: '{s}'"
                )))
            })
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'datetime' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// `datetime.strptime(date_string, format)` — always returns a naive
/// `datetime` (CPython shape). Uses chrono's format parser; unsupported
/// directives surface as ValueError rather than locale-dependent output.
fn parse_strptime(s: &str, fmt: &str) -> EvalResult {
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
        return Ok(Value::DateTime { dt, tz_offset_secs: None });
    }
    // Date-only formats: CPython still returns datetime at 00:00:00.
    if let Ok(date) = NaiveDate::parse_from_str(s, fmt) {
        let Some(time) = NaiveTime::from_hms_opt(0, 0, 0) else {
            return Err(value_error("internal: failed to build midnight time"));
        };
        return Ok(Value::DateTime { dt: NaiveDateTime::new(date, time), tz_offset_secs: None });
    }
    // Time-only formats: CPython uses date 1900-01-01.
    if let Ok(time) = NaiveTime::parse_from_str(s, fmt) {
        let Some(date) = NaiveDate::from_ymd_opt(1900, 1, 1) else {
            return Err(value_error("internal: failed to build 1900-01-01"));
        };
        return Ok(Value::DateTime { dt: NaiveDateTime::new(date, time), tz_offset_secs: None });
    }
    Err(value_error(format!("time data '{s}' does not match format '{fmt}'")))
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
/// Build a `time.struct_time` Instance (the class is seeded in
/// `InterpreterState::new`). `tm_isdst` is -1 for date/datetime (unknown).
fn build_struct_time(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    wday: u32,
    yday: u32,
) -> Value {
    let mut fields = std::collections::BTreeMap::new();
    fields.insert("tm_year".to_string(), Value::Int(i64::from(year)));
    fields.insert("tm_mon".to_string(), Value::Int(i64::from(month)));
    fields.insert("tm_mday".to_string(), Value::Int(i64::from(day)));
    fields.insert("tm_hour".to_string(), Value::Int(i64::from(hour)));
    fields.insert("tm_min".to_string(), Value::Int(i64::from(minute)));
    fields.insert("tm_sec".to_string(), Value::Int(i64::from(second)));
    fields.insert("tm_wday".to_string(), Value::Int(i64::from(wday)));
    fields.insert("tm_yday".to_string(), Value::Int(i64::from(yday)));
    fields.insert("tm_isdst".to_string(), Value::Int(-1));
    Value::Instance(crate::value::InstanceValue {
        class_name: "time.struct_time".to_string(),
        fields: crate::value::shared_fields(fields),
    })
}

/// Build a `datetime.IsoCalendarDate` Instance (ISO year, week, weekday 1-7).
fn build_isocalendar(date: NaiveDate) -> Value {
    let iso = date.iso_week();
    let mut fields = std::collections::BTreeMap::new();
    fields.insert("year".to_string(), Value::Int(i64::from(iso.year())));
    fields.insert("week".to_string(), Value::Int(i64::from(iso.week())));
    fields
        .insert("weekday".to_string(), Value::Int(i64::from(date.weekday().number_from_monday())));
    Value::Instance(crate::value::InstanceValue {
        class_name: "datetime.IsoCalendarDate".to_string(),
        fields: crate::value::shared_fields(fields),
    })
}

pub fn dispatch_date_method(
    date: NaiveDate,
    method: &str,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
) -> EvalResult {
    // `replace` accepts year/month/day keyword args; the others are argument-less.
    if method != "replace" {
        crate::eval::functions::reject_kwargs(method, kwargs)?;
    }
    match method {
        "isoformat" => Ok(Value::String(date.format("%Y-%m-%d").to_string().into())),
        // Python: Monday == 0 … Sunday == 6.
        "weekday" => Ok(Value::Int(i64::from(date.weekday().num_days_from_monday()))),
        // Python: Monday == 1 … Sunday == 7.
        "isoweekday" => Ok(Value::Int(i64::from(date.weekday().number_from_monday()))),
        // Proleptic Gregorian ordinal — day 1 is 0001-01-01, matching CPython.
        "toordinal" => Ok(Value::Int(i64::from(date.num_days_from_ce()))),
        // `datetime.IsoCalendarDate(year, week, weekday)`.
        "isocalendar" => Ok(build_isocalendar(date)),
        // `time.struct_time` with the time fields zeroed (a plain date).
        "timetuple" => Ok(build_struct_time(
            date.year(),
            date.month(),
            date.day(),
            0,
            0,
            0,
            date.weekday().num_days_from_monday(),
            date.ordinal(),
        )),
        "strftime" => {
            let fmt = arg_str("strftime", args, 0)?;
            Ok(Value::String(date.format(fmt).to_string().into()))
        }
        "replace" => {
            let b = crate::eval::functions::bind_method_params(
                method,
                args,
                kwargs,
                &["year", "month", "day"],
            )?;
            let year = comp_opt_i32(b[0].as_ref())?.unwrap_or_else(|| date.year());
            let month = comp_opt_u32(b[1].as_ref(), method)?.unwrap_or_else(|| date.month());
            let day = comp_opt_u32(b[2].as_ref(), method)?.unwrap_or_else(|| date.day());
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
    // `replace` accepts keyword components; the others are argument-less.
    if method != "replace" {
        crate::eval::functions::reject_kwargs(method, kwargs)?;
    }
    match method {
        "replace" => {
            let b = crate::eval::functions::bind_method_params(
                method,
                args,
                kwargs,
                &[
                    "year",
                    "month",
                    "day",
                    "hour",
                    "minute",
                    "second",
                    "microsecond",
                    "tzinfo",
                    "fold",
                ],
            )?;
            let year = comp_opt_i32(b[0].as_ref())?.unwrap_or_else(|| dt.year());
            let month = comp_opt_u32(b[1].as_ref(), method)?.unwrap_or_else(|| dt.month());
            let day = comp_opt_u32(b[2].as_ref(), method)?.unwrap_or_else(|| dt.day());
            let hour = comp_opt_u32(b[3].as_ref(), method)?.unwrap_or_else(|| dt.hour());
            let minute = comp_opt_u32(b[4].as_ref(), method)?.unwrap_or_else(|| dt.minute());
            let second = comp_opt_u32(b[5].as_ref(), method)?.unwrap_or_else(|| dt.second());
            let microsecond =
                comp_opt_u32(b[6].as_ref(), method)?.unwrap_or_else(|| dt.nanosecond() / 1_000);
            let tz = match b[7].as_ref() {
                None => tz_offset_secs,
                Some(Value::None) => None,
                Some(Value::TimeZone(secs)) => Some(*secs),
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "replace() tzinfo must be a datetime.timezone (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            let date = construct_naive_date(year, month, day)?;
            let time = NaiveTime::from_hms_micro_opt(hour, minute, second, microsecond)
                .ok_or_else(|| value_error("time component out of range"))?;
            Ok(Value::DateTime { dt: NaiveDateTime::new(date, time), tz_offset_secs: tz })
        }
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
        "isocalendar" => Ok(build_isocalendar(dt.date())),
        // `time.struct_time` including the wall-clock time fields.
        "timetuple" => Ok(build_struct_time(
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second(),
            dt.weekday().num_days_from_monday(),
            dt.ordinal(),
        )),
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
        // date + timedelta -> date. A date has no time, so only the timedelta's
        // whole-day count matters — and that is `timedelta.days`, the *floored*
        // day count (div_euclid), not truncation toward zero. `timedelta(hours=-1)`
        // has .days == -1, so it moves the date back a day.
        ("+", Value::Date(d), Value::TimeDelta(us))
        | ("+", Value::TimeDelta(us), Value::Date(d)) => {
            let days = i32::try_from(us.div_euclid(86_400_000_000)).ok()?;
            let result = d.checked_add_signed(Duration::days(i64::from(days)))?;
            Some(Ok(Value::Date(result)))
        }
        // date - timedelta -> date (subtract the floored whole-day count).
        ("-", Value::Date(d), Value::TimeDelta(us)) => {
            let days = i32::try_from(us.div_euclid(86_400_000_000)).ok()?;
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

/// Required `i32` component from a bound slot (year), CPython range wording.
fn comp_i32(v: Option<&Value>, func: &str, name: &str) -> Result<i32, EvalError> {
    let value = v.ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!(
            "{func}() missing required argument '{name}'"
        )))
    })?;
    i32::try_from(int_arg(value, func, 0)?).map_err(|_| value_error("year out of range"))
}

/// Required `u32` component from a bound slot (month/day).
fn comp_u32(v: Option<&Value>, func: &str, name: &str) -> Result<u32, EvalError> {
    let value = v.ok_or_else(|| {
        EvalError::from(InterpreterError::TypeError(format!(
            "{func}() missing required argument '{name}'"
        )))
    })?;
    u32::try_from(int_arg(value, func, 0)?).map_err(|_| value_error("value out of range"))
}

/// Optional `u32` component from a bound slot; `None`/absent → `None`.
fn comp_opt_u32(v: Option<&Value>, func: &str) -> Result<Option<u32>, EvalError> {
    match v {
        None | Some(Value::None) => Ok(None),
        Some(value) => Ok(Some(
            u32::try_from(int_arg(value, func, 0)?)
                .map_err(|_| value_error("value out of range"))?,
        )),
    }
}

/// Optional `i64` component from a bound slot (timedelta parts).
fn comp_opt_i64(v: Option<&Value>) -> Result<Option<i64>, EvalError> {
    match v {
        None | Some(Value::None) => Ok(None),
        Some(value) => int_arg(value, "timedelta", 0).map(Some),
    }
}

/// Optional `i32` component from a bound slot (replace year).
fn comp_opt_i32(v: Option<&Value>) -> Result<Option<i32>, EvalError> {
    match v {
        None | Some(Value::None) => Ok(None),
        Some(value) => Ok(Some(
            i32::try_from(int_arg(value, "replace", 0)?)
                .map_err(|_| value_error("year out of range"))?,
        )),
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
        kwargs: &indexmap::IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        call(func, args, kwargs)
    }
}
