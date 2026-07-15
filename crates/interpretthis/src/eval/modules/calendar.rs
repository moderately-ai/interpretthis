// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `calendar` module — the pure calendrical helpers
//! (`isleap`, `leapdays`, `weekday`, `monthrange`) and the name-table
//! constants (`month_name`/`month_abbr`/`day_name`/`day_abbr`). The name
//! tables are exposed as tuples so `calendar.month_name[1]` indexes as in
//! CPython (index 0 of the month tables is the empty string).
//!
//! `weekday`/`monthrange` return plain ints for the weekday. CPython 3.12
//! returns a `calendar.Day` IntEnum whose *repr* is `calendar.SUNDAY`; the
//! integer value is identical (`Day.SUNDAY == 6`), so equality and arithmetic
//! match — only `repr()` of the enum member differs, which we do not model.

use indexmap::IndexMap;

use crate::{
    error::{EvalResult, InterpreterError},
    state::InterpreterState,
    tools::Tools,
    value::Value,
};

const MONTH_NAME: [&str; 13] = [
    "",
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const MONTH_ABBR: [&str; 13] =
    ["", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAY_NAME: [&str; 7] =
    ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const DAY_ABBR: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Days in `month` (1-based) of `year`, accounting for February in leap years.
fn days_in_month(year: i64, month: i64) -> Option<i64> {
    let d = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => return None,
    };
    Some(d)
}

/// Weekday of `year-month-day` with Monday == 0 … Sunday == 6, via chrono.
fn weekday(year: i64, month: i64, day: i64) -> Option<i64> {
    use chrono::Datelike as _;
    let y = i32::try_from(year).ok()?;
    let m = u32::try_from(month).ok()?;
    let d = u32::try_from(day).ok()?;
    let date = chrono::NaiveDate::from_ymd_opt(y, m, d)?;
    Some(i64::from(date.weekday().num_days_from_monday()))
}

fn strings_tuple(names: &[&str]) -> Value {
    Value::Tuple(names.iter().map(|s| Value::String((*s).into())).collect())
}

fn int_arg(func: &str, args: &[Value], idx: usize) -> Result<i64, crate::error::EvalError> {
    match args.get(idx) {
        Some(Value::Int(n)) => Ok(*n),
        Some(Value::Bool(b)) => Ok(i64::from(*b)),
        _ => {
            Err(InterpreterError::TypeError(format!("{func}() requires integer arguments")).into())
        }
    }
}

pub struct CalendarModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for CalendarModule {
    fn name(&self) -> &'static str {
        "calendar"
    }

    fn constant(&self, name: &str) -> Option<Value> {
        match name {
            "month_name" => Some(strings_tuple(&MONTH_NAME)),
            "month_abbr" => Some(strings_tuple(&MONTH_ABBR)),
            "day_name" => Some(strings_tuple(&DAY_NAME)),
            "day_abbr" => Some(strings_tuple(&DAY_ABBR)),
            // Weekday index constants (MONDAY == 0 … SUNDAY == 6).
            "MONDAY" => Some(Value::Int(0)),
            "TUESDAY" => Some(Value::Int(1)),
            "WEDNESDAY" => Some(Value::Int(2)),
            "THURSDAY" => Some(Value::Int(3)),
            "FRIDAY" => Some(Value::Int(4)),
            "SATURDAY" => Some(Value::Int(5)),
            "SUNDAY" => Some(Value::Int(6)),
            _ => None,
        }
    }

    fn has_function(&self, name: &str) -> bool {
        matches!(name, "isleap" | "leapdays" | "weekday" | "monthrange")
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
            "isleap" => Ok(Value::Bool(is_leap(int_arg(func, args, 0)?))),
            // Number of leap years in range(y1, y2) — CPython's exact formula.
            "leapdays" => {
                let y1 = int_arg(func, args, 0)?;
                let y2 = int_arg(func, args, 1)?;
                let count = |y: i64| {
                    (y - 1).div_euclid(4) - (y - 1).div_euclid(100) + (y - 1).div_euclid(400)
                };
                Ok(Value::Int(count(y2) - count(y1)))
            }
            "weekday" => {
                let y = int_arg(func, args, 0)?;
                let m = int_arg(func, args, 1)?;
                let d = int_arg(func, args, 2)?;
                weekday(y, m, d).map(Value::Int).ok_or_else(|| {
                    InterpreterError::ValueError("invalid date for calendar.weekday()".into())
                        .into()
                })
            }
            // `(weekday_of_first_day, days_in_month)` — Monday == 0.
            "monthrange" => {
                let year = int_arg(func, args, 0)?;
                let month = int_arg(func, args, 1)?;
                if !(1..=12).contains(&month) {
                    return Err(InterpreterError::ValueError(format!(
                        "bad month number {month}; must be 1-12"
                    ))
                    .into());
                }
                let first = weekday(year, month, 1).ok_or_else(|| {
                    crate::error::EvalError::from(InterpreterError::ValueError(
                        "invalid year for calendar.monthrange()".into(),
                    ))
                })?;
                let days = days_in_month(year, month).ok_or_else(|| {
                    crate::error::EvalError::from(InterpreterError::ValueError(
                        "invalid month for calendar.monthrange()".into(),
                    ))
                })?;
                Ok(Value::Tuple(vec![Value::Int(first), Value::Int(days)]))
            }
            _ => Err(InterpreterError::AttributeError(format!(
                "module 'calendar' has no callable '{func}'"
            ))
            .into()),
        }
    }
}
