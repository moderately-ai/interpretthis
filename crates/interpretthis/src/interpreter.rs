// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use parking_lot::Mutex;

use crate::{
    config::InterpreterConfig, error::InterpreterError, state::InterpreterState, tools::Tools,
    value::Value,
};

/// Hard cap on submitted source length. Deep pathological input (a 100k-term
/// expression is ~200 KB) is the stack-overflow-DoS vector; a generous byte cap
/// is the cheap outer bound. Real structured-output code is far smaller.
const MAX_SOURCE_BYTES: usize = 512 * 1024;

/// Stack size for the parse and the AST Drop — the two operations whose depth
/// tracks the *whole* AST rather than the (recursion-limited) eval. Bracket
/// nesting is capped before parse and `MAX_SOURCE_BYTES` bounds the deepest
/// possible chain, so this comfortably covers the recursive `Drop` of the
/// deepest AST a legal submission can produce. Allocated lazily by `stacker`
/// only when a poll actually runs low, so it is a virtual bound, not a resident
/// cost on normal input.
const AST_STACK_SIZE: usize = 256 * 1024 * 1024;

/// Whether the maximum bracket-nesting depth of `code` exceeds `limit`.
///
/// A conservative, iterative (non-recursive) scan: it counts `([{` / `)]}`
/// outside string literals and `#` comments. It does not fully model triple
/// quotes or f-strings, but any imprecision only over-counts (rejecting some
/// otherwise-legal but absurdly bracket-dense input), never under-counts real
/// nesting — and the limit sits far above any real program. This runs before
/// the recursive-descent parser so a bracket bomb (`[[[…]]]` × 100k) is a clean
/// SyntaxError instead of a parser stack overflow.
fn bracket_depth_exceeds(code: &str, limit: usize) -> bool {
    let mut depth: usize = 0;
    let mut in_string: Option<char> = None;
    let mut escaped = false;
    let mut in_comment = false;
    for ch in code.chars() {
        if in_comment {
            if ch == '\n' {
                in_comment = false;
            }
            continue;
        }
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => in_string = Some(ch),
            '#' => in_comment = true,
            '(' | '[' | '{' => {
                depth += 1;
                if depth > limit {
                    return true;
                }
            }
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}

/// Response from a single interpreter execution.
///
/// Check `is_ok()` or `result()` for the execution outcome. Even on error,
/// `stdout` may contain captured print output from before the error.
#[derive(Debug)]
#[non_exhaustive]
pub struct InterpreterResponse {
    /// All `print()` output captured during execution.
    pub stdout: String,
    /// Error that occurred during execution, if any.
    pub error: Option<InterpreterError>,
}

impl InterpreterResponse {
    /// Returns `Ok(())` if execution succeeded, `Err` if there was an error.
    ///
    /// # Errors
    ///
    /// Returns the inner `InterpreterError` if execution failed.
    pub const fn result(&self) -> Result<(), &InterpreterError> {
        if let Some(ref err) = self.error { Err(err) } else { Ok(()) }
    }

    /// True if execution completed without error.
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        self.error.is_none()
    }
}

/// A sandboxed Python AST interpreter with tool injection.
///
/// State (variables, captured stdout, operation counter) lives behind
/// a `Mutex` so callers hold the interpreter as a shared `&Interpreter`
/// — no `&mut self` on the call surface. The mutex serialises concurrent
/// `execute` calls; that matches the interpreter's semantics anyway
/// (Python is single-threaded per execution context) but it also lets
/// observability tooling read variables / state-keys without exclusive
/// access.
pub struct Interpreter {
    state: Mutex<InterpreterState>,
    registered_tools: Tools,
}

/// Injected dependencies for [`Interpreter`].
///
/// Tools carry async handlers, so they live here rather than in
/// [`crate::InterpreterConfig`]. Build with [`Tools::new`] and
/// [`Tools::with`] / [`Tools::insert`].
pub struct InterpreterDeps {
    pub tools: Tools,
}

impl Interpreter {
    /// Create a new interpreter from its registered tools and resource
    /// limits.
    #[must_use]
    pub fn new(deps: InterpreterDeps, config: InterpreterConfig) -> Self {
        Self { state: Mutex::new(InterpreterState::new(config)), registered_tools: deps.tools }
    }

    /// Execute Python code, injecting `variables` into the
    /// interpreter's state at the start of the run.
    ///
    /// Variables are merged in *before* execution begins; any variables
    /// already present in the interpreter's state from prior `execute`
    /// calls are overwritten by entries in this map. Pass `HashMap::new()`
    /// (or `Default::default()`) when no extra bindings are needed.
    #[expect(
        clippy::await_holding_lock,
        reason = "interpreter is single-owner: the mutex serializes concurrent execute() calls \
                  on the same Arc<Interpreter>, which is the documented contract. parking_lot's \
                  `send_guard` feature lets the guard cross await safely; the lock is dropped \
                  at end of execute, so sync accessors (get_variable / state_keys / \
                  export_state / import_state) work between calls"
    )]
    pub async fn execute(
        &self,
        code: &str,
        tools: &Tools,
        variables: HashMap<String, Value>,
    ) -> InterpreterResponse {
        let mut state = self.state.lock();
        state.clear_print_buffer();
        state.reset_operations();
        state.execution_start = std::time::Instant::now();
        for (k, v) in variables {
            // Memory-limit failures during pre-execution variable
            // injection are intentionally swallowed; the limit will
            // re-trigger inside eval if the value is actually used.
            let _ = state.set_variable(&k, v);
        }

        // Merge registered tools with per-call tools (per-call takes priority)
        let effective_tools = if self.registered_tools.is_empty() {
            tools.clone()
        } else if tools.is_empty() {
            self.registered_tools.clone()
        } else {
            self.registered_tools.merged_with(tools)
        };
        let tools = &effective_tools;

        // Store source for function source extraction
        state.current_source = code.to_string();

        // Defence-in-depth against a stack-overflow DoS: a pathologically deep
        // expression (`[[[…]]]`, `f(f(f(…)))`, `1+1+…+1` × 100k) would otherwise
        // overflow the host stack — during the recursive-descent parse, during
        // eval, or during the recursive Drop of the resulting AST — as an
        // *uncatchable* SIGABRT rather than a catchable error. Bound the input
        // size and bracket-nesting depth up front (a caller-side limit usually
        // masks this, but the core must be safe on its own), evaluate the
        // numeric fast path under a recursion counter (see eval_numeric_unmetered),
        // and run the parse and the AST Drop on a large stack.
        if code.len() > MAX_SOURCE_BYTES {
            return InterpreterResponse {
                stdout: state.print_buffer.clone(),
                error: Some(InterpreterError::Syntax(format!(
                    "source exceeds the {MAX_SOURCE_BYTES}-byte limit"
                ))),
            };
        }
        if bracket_depth_exceeds(code, state.config.max_recursion_depth as usize) {
            return InterpreterResponse {
                stdout: state.print_buffer.clone(),
                error: Some(InterpreterError::Syntax("expression nesting is too deep".to_string())),
            };
        }

        // Parse (on a large stack: deeply nested source recurses the parser).
        let stmts = match stacker::grow(AST_STACK_SIZE, || crate::parser::parse(code)) {
            Ok(s) => s,
            Err(e) => {
                return InterpreterResponse { stdout: state.print_buffer.clone(), error: Some(e) };
            }
        };

        // Evaluate, then Drop the (possibly very deep) AST on a large stack so a
        // long operator chain / deep nesting can't overflow during recursive Drop.
        let eval_result = crate::eval::eval_body(&mut state, &stmts, tools).await;
        stacker::grow(AST_STACK_SIZE, move || drop(stmts));
        match eval_result {
            Ok(_) => {
                // CPython runs a generator's `finally`/`with` cleanup when
                // it is finalised at interpreter shutdown; finalise any
                // still-suspended generators at the end of a successful
                // run to match that (see `finalize_generators`).
                crate::eval::functions::finalize_generators(&mut state, tools).await;
                InterpreterResponse { stdout: state.print_buffer.clone(), error: None }
            }
            Err(crate::error::EvalError::Signal(crate::error::ControlFlow::Return(_))) => {
                InterpreterResponse {
                    stdout: state.print_buffer.clone(),
                    error: Some(InterpreterError::Runtime("'return' outside function".into())),
                }
            }
            Err(crate::error::EvalError::Signal(crate::error::ControlFlow::Break)) => {
                InterpreterResponse {
                    stdout: state.print_buffer.clone(),
                    error: Some(InterpreterError::Runtime("'break' outside loop".into())),
                }
            }
            Err(crate::error::EvalError::Signal(crate::error::ControlFlow::Yield(_))) => {
                InterpreterResponse {
                    stdout: state.print_buffer.clone(),
                    error: Some(InterpreterError::Runtime("'yield' outside function".into())),
                }
            }
            Err(crate::error::EvalError::Signal(crate::error::ControlFlow::Continue)) => {
                InterpreterResponse {
                    stdout: state.print_buffer.clone(),
                    error: Some(InterpreterError::Runtime(
                        "'continue' not properly in loop".into(),
                    )),
                }
            }
            Err(crate::error::EvalError::Interpreter(e)) => {
                InterpreterResponse { stdout: state.print_buffer.clone(), error: Some(e) }
            }
            Err(crate::error::EvalError::Exception(exc)) => {
                // The (at line N) suffix is set on ExceptionValue's
                // stamped_line side-field by stamp_line — kept off the
                // user-visible `message` so `print(e)` / `str(e)`
                // inside the script don't bleed the debug stamp. At
                // this boundary, the host pipeline wants the line
                // info, so append it to the rendered message here.
                let message = match exc.stamped_line {
                    Some(line) => format!("{} (at line {line})", exc.message),
                    None => exc.message,
                };
                InterpreterResponse {
                    stdout: state.print_buffer.clone(),
                    error: Some(InterpreterError::PythonException {
                        type_name: exc.type_name,
                        message,
                    }),
                }
            }
        }
    }

    /// Get a variable from the interpreter state by name.
    ///
    /// Returns a cloned `Value` because the state mutex would otherwise
    /// keep a lock guard alive past the return.
    #[must_use]
    pub fn get_variable(&self, key: &str) -> Option<Value> {
        self.state.lock().get_variable(key).cloned()
    }

    /// Get user-visible state keys (excludes internal keys).
    #[must_use]
    pub fn state_keys(&self) -> Vec<String> {
        self.state.lock().state_keys()
    }

    /// Bytes the interpreter believes it has accounted for. Tracks
    /// `Value`-tree size: per-slot enum overhead + container headers +
    /// string headers + heap payload. Gates `max_memory_bytes`.
    ///
    /// Not the same as RSS — short heap allocs and bookkeeping the
    /// interpreter never sees aren't here. For an allocator-real
    /// number use `resident_bytes`. For drift detection use this.
    #[must_use]
    pub fn accounted_bytes(&self) -> usize {
        self.state.lock().memory_used_bytes
    }

    /// Backwards-compatible alias for `accounted_bytes`. The original
    /// name was misleading: it isn't bytes the allocator handed out,
    /// it's the interpreter's accounting counter.
    #[must_use]
    #[deprecated(since = "0.2.0", note = "renamed to `accounted_bytes`")]
    pub fn memory_used_bytes(&self) -> usize {
        self.accounted_bytes()
    }

    /// Allocator-reported resident bytes when the bench-jemalloc
    /// feature is on; falls back to `accounted_bytes` otherwise.
    /// Reads via `tikv_jemalloc_ctl::stats::resident` — eventually
    /// consistent (jemalloc updates the stat every epoch). Use for
    /// capacity planning, not for hot-loop accounting.
    #[must_use]
    pub fn resident_bytes(&self) -> usize {
        #[cfg(feature = "bench-alloc-jemalloc")]
        {
            use tikv_jemalloc_ctl::{epoch, stats};
            // Advance the epoch so subsequent reads see fresh stats;
            // jemalloc snapshots on epoch advance, not on read.
            let _ = epoch::advance();
            stats::resident::read().unwrap_or_else(|_| self.accounted_bytes())
        }
        #[cfg(not(feature = "bench-alloc-jemalloc"))]
        {
            self.accounted_bytes()
        }
    }

    /// Serialize interpreter variable/class state to a versioned byte blob.
    ///
    /// Internal keys and [`crate::Value::LazyProxy`] values are omitted — they
    /// are not meaningful across a resume boundary. Signing/encryption is a
    /// host concern.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON serialization of the remaining state fails.
    pub fn export_state(&self) -> Result<Vec<u8>, InterpreterError> {
        crate::serialize::export_state(&self.state.lock())
    }

    /// Deserialize interpreter state from bytes.
    ///
    /// # Errors
    ///
    /// Returns an `InterpreterError` if the bytes are not a valid state
    /// payload or if any contained value fails to deserialize.
    pub fn import_state(&self, data: &[u8]) -> Result<(), InterpreterError> {
        crate::serialize::import_state(&mut self.state.lock(), data)
    }
}
