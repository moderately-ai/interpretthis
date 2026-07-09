// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Runner for the vendored CPython test corpus under `cpython_vendored/`.
//!
//! Each `cpython_vendored/<name>.py` is an adaptation of a CPython 3.12 stdlib
//! test file with the `unittest` shell stripped (see the directory's README.md
//! for provenance and adaptation rules). The runner executes each file twice —
//! once through the host `python3.12` interpreter, once through the interpretthis
//! interpreter — and parses both stdouts for the `X/Y passed` line that each
//! adapted file's runner block emits.
//!
//! Reporting is **per-file, informational**. The Rust-level test asserts only
//! on **infrastructure** failures:
//!   * Host `python3` / `python3.12` not invokable → the entire test no-ops (asserts pass; a notice
//!     is captured into the test's report string) — keeps the binary green on machines without a
//!     host Python.
//!   * Vendored file unreadable / host python exits non-zero / no pass-line in host stdout →
//!     infrastructure bug, asserts fail.
//!
//! The interpreter-pass / host-pass metric is reported via `assert!` messages
//! that pass — the message body is captured by `cargo nextest run --no-capture`
//! as the test's stdout. Promoting the metric to a blocking assertion happens
//! later, once the pass-rate trend is monotone (per the parity plan's
//! foundation #3).
//!
//! ## Runtime walk vs. compile-time walk
//!
//! We do a **runtime directory walk** (well — a fixed `#[tokio::test]` per
//! file). Compile-time discovery via `include_dir!` was rejected because:
//!   * The set of vendored files is small (~7) and changes only when a human adds a new adaptation,
//!     not on every build.
//!   * Per-file `#[tokio::test]` functions give nextest individually-named test cases
//!     (`cpython_imported_test_dict_adapted`, etc.) which slot cleanly into nextest's per-test
//!     parallelism and per-test retry/report surfaces; a single test that iterates a directory
//!     would collapse seven informational reports into one and lose that granularity.
//!   * `include_dir!` would embed the Python sources into the test binary, bloating the test binary
//!     by ~50 KB of source we already read from disk anyway.

use std::{path::PathBuf, process::Command, sync::OnceLock};

use interpretthis::{Interpreter, InterpreterConfig, InterpreterDeps, Tools};

/// Absolute path to the `cpython_vendored/` directory inside this crate's
/// integration-test tree. Resolved via `env!("CARGO_MANIFEST_DIR")` so the
/// runner works regardless of the working directory `cargo nextest` invokes
/// us from.
fn vendored_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("integration")
        .join("cpython_vendored")
}

/// The python3 binary name to invoke. Pinned to `python3.12` per the parity
/// plan; falls back to `python3` only if `python3.12` isn't on PATH. Probed
/// once per test process.
fn host_python() -> Option<&'static str> {
    static AVAILABLE: OnceLock<Option<&'static str>> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        ["python3.12", "python3"].into_iter().find(|candidate| {
            Command::new(candidate).arg("--version").output().is_ok_and(|out| out.status.success())
        })
    })
}

/// Pass-count parsed from the trailing `"X/Y passed"` line emitted by every
/// vendored file's runner block.
#[derive(Debug, Clone, Copy)]
struct PassRate {
    passed: u32,
    total: u32,
}

/// Parse the trailing `"X/Y passed"` line. Searches from the last line
/// backwards so any preceding `FAIL <name>:` lines emitted by the runner
/// block don't confuse the parser. Returns `None` if no recognisable line
/// is found.
fn parse_pass_line(stdout: &str) -> Option<PassRate> {
    for line in stdout.lines().rev() {
        let line = line.trim();
        let Some(rest) = line.strip_suffix(" passed") else { continue };
        let (passed_str, total_str) = rest.split_once('/')?;
        let passed = passed_str.parse::<u32>().ok()?;
        let total = total_str.parse::<u32>().ok()?;
        if passed > total {
            return None;
        }
        return Some(PassRate { passed, total });
    }
    None
}

/// Run a source file under the host python3 interpreter with the same env
/// discipline the parity corpus runner uses (`PYTHONHASHSEED=0`,
/// `PYTHONIOENCODING=utf-8`, `LC_ALL=C.UTF-8`). Returns `Ok((stdout, success))`
/// or an `Err` describing the invocation failure.
fn run_host_python(binary: &str, source_path: &std::path::Path) -> Result<(String, bool), String> {
    let output = Command::new(binary)
        .arg(source_path)
        .env("PYTHONHASHSEED", "0")
        .env("PYTHONIOENCODING", "utf-8")
        .env("LC_ALL", "C.UTF-8")
        .output()
        .map_err(|err| {
            format!("failed to invoke host {binary} on {}: {err}", source_path.display())
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok((stdout, output.status.success()))
}

/// Run `source` through a fresh interpretthis instance. Returns the captured
/// stdout and (separately) any structured error, formatted as a debug string
/// for inclusion in the per-file report.
async fn run_interpreter(source: &str) -> (String, Option<String>) {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let response = interp.execute(source, &Tools::new(), std::collections::HashMap::new()).await;
    let error_message = response.error.as_ref().map(|err| format!("{err:?}"));
    (response.stdout, error_message)
}

/// Outcome of running one vendored file end-to-end. Returned as a string the
/// per-file `#[tokio::test]` body asserts on (either as a success message or
/// a failure reason).
struct VendoredReport {
    summary: String,
    failing_test_lines: Vec<String>,
}

/// End-to-end driver for a single vendored file. Returns `Ok(report)` on
/// every infrastructure-clean run (regardless of interpretthis pass rate); the
/// caller turns the report into a visible message via `assert!`. Returns
/// `Err(reason)` only on genuine infrastructure failures (file missing,
/// host python crashed, host pass-line absent).
async fn drive(file_name: &str) -> Result<Option<VendoredReport>, String> {
    let path = vendored_dir().join(file_name);
    if !path.exists() {
        return Err(format!("vendored file not found: {}", path.display()));
    }
    let source = std::fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    if source.is_empty() {
        return Err(format!("vendored file {file_name} is empty"));
    }
    let Some(binary) = host_python() else {
        return Ok(None);
    };
    let (host_stdout, host_ok) = run_host_python(binary, &path)?;
    if !host_ok {
        return Err(format!(
            "host {binary} exited non-zero on {file_name}; stdout:\n{host_stdout}"
        ));
    }
    let Some(host_rate) = parse_pass_line(&host_stdout) else {
        return Err(format!(
            "host {binary} did not emit an `X/Y passed` line for {file_name}; stdout was:\n{host_stdout}"
        ));
    };
    let (cat_stdout, cat_error) = run_interpreter(&source).await;
    let cat_rate = parse_pass_line(&cat_stdout);
    let cat_summary = match (cat_rate, cat_error.as_deref()) {
        (Some(rate), _) => format!("{}/{} passed on interpretthis", rate.passed, rate.total),
        (None, Some(err)) => {
            format!("0/{} passed on interpretthis (interpreter errored: {err})", host_rate.total)
        }
        (None, None) => format!(
            "0/{} passed on interpretthis (no pass-line in stdout; last line: {:?})",
            host_rate.total,
            cat_stdout.lines().last().unwrap_or("")
        ),
    };
    let summary = format!(
        "cpython_imported::{file_name}: host={}/{} passed; {cat_summary}",
        host_rate.passed, host_rate.total
    );
    let failing_test_lines =
        cat_stdout.lines().filter(|line| line.starts_with("FAIL ")).map(str::to_owned).collect();
    Ok(Some(VendoredReport { summary, failing_test_lines }))
}

/// Assert-format helper. Builds the message shown by `cargo nextest run
/// --no-capture` when an infra failure happens, or attached to the trivially-
/// satisfied `assert!(true, ...)` on the success path. The metric is in the
/// message body either way.
fn render(file_name: &str, outcome: &Result<Option<VendoredReport>, String>) -> String {
    match outcome {
        Ok(Some(report)) => {
            if report.failing_test_lines.is_empty() {
                report.summary.clone()
            } else {
                let mut out = report.summary.clone();
                for line in &report.failing_test_lines {
                    out.push_str("\n    ");
                    out.push_str(line);
                }
                out
            }
        }
        Ok(None) => {
            format!("cpython_imported::{file_name}: SKIP (host python3 not available)")
        }
        Err(reason) => reason.clone(),
    }
}

/// Surface the per-file report in nextest's `--no-capture` output. We deny
/// `print_stdout` workspace-wide because it's a real bug class in production
/// code, but this runner exists *to print a metric* — the interpreter-pass /
/// host-pass count is the foundation #3 deliverable, and the only way nextest
/// shows it is via stdout. The `#[expect]` will surface as `unfulfilled` if we
/// ever switch to a structured-test-report mechanism that doesn't need stdout.
#[expect(
    clippy::print_stdout,
    reason = "informational metric for CPython-imported corpus; see module docstring"
)]
fn report(message: &str) {
    println!("{message}");
}

#[tokio::test]
async fn cpython_imported_test_dict_adapted() {
    let file_name = "test_dict_adapted.py";
    let outcome = drive(file_name).await;
    let message = render(file_name, &outcome);
    assert!(outcome.is_ok(), "{message}");
    report(&message);
}

#[tokio::test]
async fn cpython_imported_test_set_adapted() {
    let file_name = "test_set_adapted.py";
    let outcome = drive(file_name).await;
    let message = render(file_name, &outcome);
    assert!(outcome.is_ok(), "{message}");
    report(&message);
}

#[tokio::test]
async fn cpython_imported_test_list_adapted() {
    let file_name = "test_list_adapted.py";
    let outcome = drive(file_name).await;
    let message = render(file_name, &outcome);
    assert!(outcome.is_ok(), "{message}");
    report(&message);
}

#[tokio::test]
async fn cpython_imported_test_int_adapted() {
    let file_name = "test_int_adapted.py";
    let outcome = drive(file_name).await;
    let message = render(file_name, &outcome);
    assert!(outcome.is_ok(), "{message}");
    report(&message);
}

#[tokio::test]
async fn cpython_imported_test_float_adapted() {
    let file_name = "test_float_adapted.py";
    let outcome = drive(file_name).await;
    let message = render(file_name, &outcome);
    assert!(outcome.is_ok(), "{message}");
    report(&message);
}

#[tokio::test]
async fn cpython_imported_test_str_adapted() {
    let file_name = "test_str_adapted.py";
    let outcome = drive(file_name).await;
    let message = render(file_name, &outcome);
    assert!(outcome.is_ok(), "{message}");
    report(&message);
}

#[tokio::test]
async fn cpython_imported_test_bytes_adapted() {
    let file_name = "test_bytes_adapted.py";
    let outcome = drive(file_name).await;
    let message = render(file_name, &outcome);
    assert!(outcome.is_ok(), "{message}");
    report(&message);
}
