// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Differential parity corpus runner.
//!
//! The companion `build.rs` walks `tests/integration/parity_corpus/` at
//! compile time and emits one `#[test] fn <stem>()` per `.py` snippet into
//! `$OUT_DIR/parity_corpus_generated.rs`. We `include!` that file inside the
//! `parity` module below, so the resulting test names are
//! `parity::<topic>::<name>` — visible per-test in `cargo nextest run`.
//!
//! Each generated test calls [`run_parity_test`] with the snippet's source
//! and `module_path!()`. The runner:
//!
//! 1. Probes for a usable `python3` (exactly 3.12.x per the CONFORMANCE.md-pinned contract). When
//!    no suitable interpreter is available, the test is skipped with an explanatory `eprintln!` —
//!    *not* failed. Parity is advisory in dev, mandatory in CI, and the CI environment is
//!    provisioned with the pinned interpreter.
//! 2. Runs the snippet through CPython with deterministic env (`PYTHONHASHSEED=0`,
//!    `PYTHONIOENCODING=utf-8`, `LC_ALL=C.UTF-8`).
//! 3. Runs the snippet through interpretthis on a private Tokio runtime (the test fn
//!    itself is synchronous so the build.rs emitter can stay agnostic of `#[tokio::test]`).
//! 4. Asserts equal exit-success and byte-equal stdout, reporting a unified diff on mismatch.

#![expect(
    clippy::print_stderr,
    reason = "test-only runner: skip-path notices go to stderr so nextest surfaces them next to the test name"
)]
#![expect(
    clippy::panic,
    reason = "this module's helpers are called only from generated `#[test]` fns; an `assert!`/`panic!` here IS the test-failure signal"
)]

use std::{
    collections::HashMap,
    fmt::Write as _,
    process::{Command, Output},
    sync::OnceLock,
};

use interpretthis::{Interpreter, InterpreterConfig, InterpreterDeps, Tools};

/// Drive a single `.py` snippet through both engines and assert parity.
///
/// `module_path` is `module_path!()` from the generated test, used to derive
/// the topic path shown in failure messages (e.g.
/// `integration::parity_corpus_runner::parity::dicts`). `snippet_name` is the
/// `.py` file stem.
pub fn run_parity_test(module_path: &'static str, snippet_name: &str, code: &str) {
    let topic = topic_from_module_path(module_path);
    let label = format!("parity::{topic}::{snippet_name}");

    let Some(python) = usable_python() else {
        eprintln!(
            "[{label}] skipped: no python3.12.x on PATH (parity is advisory in dev, mandatory in CI; see CONFORMANCE.md#reference-python-version)"
        );
        return;
    };

    let cpython_output = match run_cpython(&python, code) {
        Ok(output) => output,
        Err(err) => {
            eprintln!("[{label}] skipped: failed to invoke {python}: {err}");
            return;
        }
    };

    let our_response = run_interpreter(code);

    let cpython_stdout = String::from_utf8_lossy(&cpython_output.stdout).into_owned();
    let cpython_ok = cpython_output.status.success();
    let our_ok = our_response.error.is_none();

    assert!(
        our_ok == cpython_ok,
        "[{label}] exit-status mismatch (cpython ok={cpython_ok}, ours ok={our_ok})\n\
         --- cpython stdout ---\n{cpython_stdout}\n\
         --- cpython stderr ---\n{}\n\
         --- ours stdout ---\n{}\n\
         --- ours error ---\n{}\n",
        String::from_utf8_lossy(&cpython_output.stderr),
        our_response.stdout,
        our_response.error.as_ref().map(ToString::to_string).unwrap_or_default(),
    );

    // On the happy path both engines must emit byte-identical stdout. On the
    // error path the stdout that leaked before the raise still has to match —
    // a snippet that prints two lines before raising should print the same
    // two lines through either engine.
    assert!(
        our_response.stdout == cpython_stdout,
        "[{label}] stdout byte-mismatch vs cpython\n\
         --- expected (cpython) ---\n{cpython_stdout}\
         --- actual (ours) ---\n{}\
         --- unified diff (expected vs actual) ---\n{}",
        our_response.stdout,
        unified_diff(&cpython_stdout, &our_response.stdout),
    );

    // Error-wording parity. When both engines fail, CPython's stderr
    // ends with `<ErrorType>: <message>` after the traceback; our
    // rendered error is `<ErrorType>: <message> (at line N)` after the
    // recent prefix-realignment + line-stamping. Strip our line-stamp
    // suffix and compare against CPython's last stderr line.
    //
    // This closes the runner's old exit-status-only blind spot —
    // until this landed, two engines could fail with totally
    // divergent error wording and the test passed silently. Now every
    // parity_corpus snippet that fails in both engines automatically
    // verifies error-wording byte parity.
    if !our_ok && !cpython_ok {
        let cpython_stderr = String::from_utf8_lossy(&cpython_output.stderr);
        let cpython_err = extract_last_nonblank_line(&cpython_stderr);
        let our_err_raw = our_response.error.as_ref().map(ToString::to_string).unwrap_or_default();
        let our_err_base = strip_line_stamp(&our_err_raw);

        // CPython sometimes emits hint lines (e.g. for ImportError) or
        // PEP-657 carets that follow the main error line. Skip the
        // diff when CPython's last line doesn't look like a typed
        // error (`Type: message` shape) — diffing carets / hint
        // arrows against our errors would be noise, not signal.
        if looks_like_typed_error(cpython_err) {
            assert!(
                our_err_base == cpython_err.trim(),
                "[{label}] error-wording mismatch vs cpython\n\
                 --- expected (cpython last stderr line) ---\n{}\n\
                 --- actual (ours, line-stamp stripped) ---\n{}\n\
                 --- raw ours ---\n{}\n\
                 --- raw cpython stderr ---\n{}\n",
                cpython_err.trim(),
                our_err_base,
                our_err_raw,
                cpython_stderr,
            );
        }
    }
}

/// Last non-blank line of `text`. CPython's stderr always ends with
/// the typed error line (`TypeError: ...`) after the traceback frames.
fn extract_last_nonblank_line(text: &str) -> &str {
    text.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("")
}

/// Remove our ` (at line N)` line-stamp suffix from an error message
/// so the base can be byte-compared against CPython (which carries
/// the line via a separate traceback frame, not a suffix). Returns
/// the input unchanged if no stamp is present.
fn strip_line_stamp(msg: &str) -> &str {
    msg.split(" (at line ").next().unwrap_or(msg).trim_end()
}

/// True when `line` looks like a CPython typed-error rendering
/// (`Type: message`). Filters out PEP-657 carets / hint arrows /
/// blank padding before the main error line, so we only diff what
/// CPython considers the error proper.
fn looks_like_typed_error(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.find(':').is_some_and(|colon_idx| {
        let type_part = &trimmed[..colon_idx];
        // Type names: alphabetic start, no spaces, optionally with a
        // dotted qualifier like `json.decoder.JSONDecodeError`.
        !type_part.is_empty()
            && type_part.chars().next().is_some_and(char::is_alphabetic)
            && !type_part.contains(' ')
            && trimmed[colon_idx..].starts_with(": ")
    })
}

/// Strip the leading `integration::parity_corpus_runner::parity::` prefix
/// from `module_path!()`, returning just the topic chain (e.g. `dicts`,
/// `modules::collections`). The prefix layout is fixed by the integration
/// binary name + the `mod parity` wrapper in this file's `include!`.
fn topic_from_module_path(module_path: &str) -> String {
    // Forms observed in practice:
    //   integration::parity_corpus_runner::parity::dicts
    //   integration::parity_corpus_runner::parity::modules::collections
    // Split off everything up to and including `parity::`.
    let needle = "parity::";
    module_path.find(needle).map_or_else(
        || module_path.to_string(),
        |idx| module_path[idx + needle.len()..].to_string(),
    )
}

/// Run `code` through a fresh interpreter on a one-shot Tokio runtime.
fn run_interpreter(code: &str) -> interpretthis::InterpreterResponse {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => panic!("failed to build tokio runtime: {err}"),
    };
    runtime.block_on(async {
        let interp =
            Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
        interp.execute(code, &Tools::new(), HashMap::new()).await
    })
}

/// Invoke CPython on `code` with the deterministic env the corpus contract
/// requires. Returns the raw `Output` so callers can inspect status, stdout,
/// and stderr.
fn run_cpython(python: &str, code: &str) -> std::io::Result<Output> {
    Command::new(python)
        .arg("-c")
        .arg(code)
        .env("PYTHONHASHSEED", "0")
        .env("PYTHONIOENCODING", "utf-8")
        .env("LC_ALL", "C.UTF-8")
        .output()
}

/// Resolve a usable Python interpreter, preferring `python3.12` if present
/// and falling back to whatever `python3` reports as long as its minor is
/// exactly 12. CONFORMANCE.md pins parity to 3.12.x; accepting a different
/// minor would silently shift the baseline.
///
/// Memoised so the per-process check happens once; subsequent calls return
/// the cached path. `None` means no suitable interpreter is available and
/// callers should treat their test as skipped.
fn usable_python() -> Option<String> {
    static RESOLVED: OnceLock<Option<String>> = OnceLock::new();
    RESOLVED
        .get_or_init(|| {
            for candidate in ["python3.12", "python3"] {
                if let Some((major, minor)) = probe_version(candidate) {
                    if major == 3 && minor == 12 {
                        return Some(candidate.to_string());
                    }
                }
            }
            None
        })
        .clone()
}

/// Parse `python3 --version` output (e.g. `Python 3.12.4`) into a (major,
/// minor) tuple. Returns `None` if the binary is missing or the output is
/// not in the expected shape.
fn probe_version(binary: &str) -> Option<(u32, u32)> {
    let output = Command::new(binary).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let trimmed = stdout.trim();
    let stderr = String::from_utf8(output.stderr).ok()?;
    // `python --version` historically printed to stderr; modern Python uses
    // stdout. Accept either.
    let source = if trimmed.is_empty() { stderr.trim().to_string() } else { trimmed.to_string() };
    let rest = source.strip_prefix("Python ")?;
    let mut parts = rest.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// Render a minimal unified-style diff between `expected` and `actual` so
/// snippet failures point at the offending line without dragging a `similar`
/// dependency into the crate.
fn unified_diff(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let max = expected_lines.len().max(actual_lines.len());
    let mut out = String::new();
    for idx in 0..max {
        match (expected_lines.get(idx), actual_lines.get(idx)) {
            (Some(exp), Some(act)) if exp == act => {
                let _ = writeln!(out, " {exp}");
            }
            (Some(exp), Some(act)) => {
                let _ = writeln!(out, "-{exp}");
                let _ = writeln!(out, "+{act}");
            }
            (Some(exp), None) => {
                let _ = writeln!(out, "-{exp}");
            }
            (None, Some(act)) => {
                let _ = writeln!(out, "+{act}");
            }
            (None, None) => break,
        }
    }
    out
}

/// Generated test cases live under this module so their full paths are
/// `crate::parity_corpus_runner::parity::<topic>::<snippet>` — matching the
/// `parity::<topic>::<name>` shape the task brief calls for.
pub mod parity {
    include!(concat!(env!("OUT_DIR"), "/parity_corpus_generated.rs"));
}
