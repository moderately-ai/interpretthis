// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{sync::Arc, time::Instant};

use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Expr, Stmt};
use tokio::sync::Semaphore;

use crate::{
    config::InterpreterConfig,
    value::{ClassValue, Value},
};

/// Suspended generator function frame (true yield/resume, not eager buffer).
#[derive(Debug, Clone)]
pub struct GeneratorFrame {
    #[allow(dead_code)]
    pub func_name: String,
    pub source: String,
    /// Body AST (shared with function_bodies cache).
    pub body: std::sync::Arc<Vec<rustpython_parser::ast::Stmt>>,
    /// Names this frame may touch (for capture/restore).
    pub touched: Vec<String>,
    /// Frame-local bindings at last suspend (or initial args).
    pub locals: rustc_hash::FxHashMap<String, crate::value::Value>,
    pub started: bool,
    pub finished: bool,
    pub closed: bool,
    /// Result of the `yield` expression on resume (`send` value).
    pub send_value: crate::value::Value,
    /// When true, the next `yield` expression returns `send_value` instead of suspending.
    pub resume_at_yield: bool,
    /// Exception injected by `generator.throw(...)`: raised at the suspended
    /// `yield` on resume (so the generator's own `try/except` can catch it)
    /// instead of the yield returning `send_value`.
    pub pending_throw: Option<Box<crate::value::ExceptionValue>>,
    /// Next top-level statement index in `body`.
    pub stmt_index: usize,
    /// Nested for-loop / yield-from resume states (innermost last).
    pub for_stack: Vec<GeneratorForState>,
    /// Resume index within a suspended top-level `while` loop's body. `Some(i)`
    /// means "re-enter the while body at statement `i` without re-checking the
    /// condition" (we suspended on a `yield` there); `None` means no while is
    /// mid-suspension. Only top-level whiles with direct-statement yields take
    /// this path (see `generator_suspendable`); anything else stays eager.
    pub while_resume: Option<usize>,
}

/// Resume state for a `for` loop that suspended on `yield` in its body.
#[derive(Debug, Clone)]
pub struct GeneratorForState {
    pub items: Arc<Vec<crate::value::Value>>,
    /// Index of the current item (the one whose body is in progress).
    pub pos: usize,
    /// Next statement index in the for-body after a yield resume.
    pub body_index: usize,
    /// Simple name target only (`for x in ...`). Empty => yield-from drain.
    pub target: String,
}

/// Internal state of the interpreter — variables, print buffer, limits.
pub struct InterpreterState {
    pub variables: FxHashMap<String, Value>,
    /// User-defined classes, keyed by class name. A `Value::Class` variable is
    /// just a handle naming an entry here; methods and class attributes live in
    /// the [`ClassValue`] so instances stay cheap and methods are stored once.
    pub classes: FxHashMap<String, ClassValue>,
    pub print_buffer: String,
    /// Parse cache for user-function body AST nodes, keyed by name.
    /// `rustpython_parser::ast` is not `Serialize`, so the canonical source
    /// of truth is `FunctionDef::source`; this map is populated at
    /// definition time and re-populated from `source` on state import so
    /// hot-path calls don't re-parse. Bodies are stored behind `Arc` so
    /// `call_user_function` clones a pointer rather than the full AST
    /// vector per call — every recursive frame would otherwise hold a
    /// freshly cloned body across `execute_body(...).await`, scaling
    /// per-frame heap/stack cost with function body size.
    pub function_bodies: FxHashMap<String, Arc<Vec<Stmt>>>,
    /// Stores lambda body AST nodes, keyed by a unique id. `Arc` for
    /// the same reason as `function_bodies`.
    pub lambda_bodies: FxHashMap<String, Arc<Expr>>,
    /// The current source code being executed (used to extract function source).
    pub current_source: String,
    /// Per-body source stack. Pushed by `call_user_function` /
    /// `call_lambda` before executing the body, popped after. When
    /// non-empty, `eval_stmt`'s line-stamp uses the top of the stack
    /// instead of `current_source` — so an error inside a function
    /// body persisted from a prior `execute()` call points at the
    /// function definition's source line, not at line 1 of the
    /// current `execute()` call's source.
    pub body_source_stack: Vec<String>,
    pub operations_count: u64,
    /// Wall-clock start time for execution timeout tracking.
    pub execution_start: Instant,
    /// Active `decimal` context precision (CPython default 28).
    pub decimal_prec: i64,
    /// Approximate memory used by interpreter state, in bytes.
    pub memory_used_bytes: usize,
    /// Current nested call depth. Bumped on entry to each user function
    /// / lambda frame and decremented on exit. Guarded against
    /// `config.max_recursion_depth`.
    pub call_depth: u32,
    /// Method-call frame stack. Pushed by `call_method` on entry,
    /// popped on exit. Carries the defining class for the executing
    /// method plus the current `self`, so zero-arg `super()` can pick
    /// up both without threading them through every helper. Transient
    /// execution state — not part of the serialized checkpoint.
    pub method_frame_stack: Vec<MethodFrame>,
    /// Yield-buffer stack for generator functions (Track C). Each
    /// entry is the buffer of yielded values for one generator
    /// frame; pushed on entry to a generator body, popped on exit and
    /// wrapped as a `Value::List`. The stack lets nested generator
    /// calls keep their yields separate. Transient — not serialized.
    pub yield_stack: Vec<Vec<Value>>,
    /// Active suspended generators keyed by `Value::Generator::id`.
    pub generators: rustc_hash::FxHashMap<u64, GeneratorFrame>,
    /// Stack of generators currently being stepped (supports yield-from).
    pub active_generator_stack: Vec<u64>,
    /// Active-exception stack for exception-handler bodies. Pushed by
    /// `try_match_handlers` on entry to a matching handler, popped on
    /// exit. Bare `raise` re-raises the top; a new exception raised
    /// inside a handler picks the top as its implicit
    /// `__context__` (CPython's "during handling of the above
    /// exception, another exception occurred" chaining). Transient —
    /// not part of the serialized checkpoint.
    pub active_exception_stack: Vec<crate::value::ExceptionValue>,
    /// Cursor positions for `Value::Lazy` iterators (generator
    /// results), keyed by the variant's `cursor_id`. A position of N
    /// means the next item to yield is `items[N]`; when N >= len,
    /// the iterator is exhausted.
    pub lazy_cursors: FxHashMap<u64, usize>,
    /// Monotonic id source for `Value::Lazy::cursor_id`. Bumped on
    /// each new generator-iterator allocation; never reused so two
    /// concurrent generators can't collide.
    pub next_cursor_id: u64,
    /// Shared storage for `nonlocal`-bound variables, keyed by the
    /// owning `FunctionDef::nonlocal_cell_id`. Every call to the same
    /// function sees and updates the same cell, so `n += 1` inside an
    /// inner function persists across calls — matching CPython's
    /// reference semantics for nonlocal captures.
    pub nonlocal_cells: FxHashMap<u64, FxHashMap<String, crate::value::Value>>,
    /// Monotonic id source for `FunctionDef::nonlocal_cell_id`. Bumped
    /// each time a new function-with-nonlocal is defined.
    pub next_nonlocal_cell_id: u64,
    /// Per-frame "cells this frame owns" — pushed on entry to a
    /// user-function call and popped on exit. When a nested `def`
    /// declares `nonlocal x`, the enclosing frame registers `x` →
    /// (cell_id) here. Subsequent assignments to `x` in this frame
    /// flow through `set_variable` and write-through to
    /// `nonlocal_cells[cell_id][x]` so the inner sees the outer's
    /// live value on its next call (CPython binds inner's nonlocal
    /// to outer's actual cell object; this is the equivalent
    /// write-through). Transient — not part of the serialized
    /// checkpoint.
    pub frame_cell_owners: Vec<FxHashMap<String, u64>>,
    pub config: InterpreterConfig,
    /// Shared semaphore for concurrent tool calls.
    pub tool_semaphore: Arc<Semaphore>,
}

/// One entry on `InterpreterState::method_frame_stack`. The
/// `defining_class` is the class on whose body the executing method was
/// defined — `super()` resumes from the next MRO slot after this one.
/// `self_value` is the bound receiver at frame-push time (captured so
/// zero-arg `super()` can construct a proxy without re-reading from
/// variables). `self_local_name` is the local variable name in the
/// current scope that holds `self`; after a `super().<method>(...)`
/// call mutates the instance, the result is written back to this
/// variable so subsequent statements in the calling method see the
/// updated state.
#[derive(Debug, Clone)]
pub struct MethodFrame {
    pub defining_class: String,
    pub self_value: Value,
    pub self_local_name: Option<String>,
}

impl InterpreterState {
    pub fn new(config: InterpreterConfig) -> Self {
        // CPython binds `__name__ = "__main__"` at module scope when a file is
        // executed directly; the standard `if __name__ == "__main__":` guard at
        // the bottom of every script depends on it. This interpreter
        // executes user code as a single top-level "module", so bind the same
        // default. `__name__` is not in DANGEROUS_NAMES or BLOCKED_ATTRIBUTES
        // (it's a benign dunder); `state_keys()` filters underscore-prefixed
        // names so it stays out of user-visible state listings.
        let mut variables = FxHashMap::default();
        variables.insert("__name__".to_string(), Value::String("__main__".into()));
        Self {
            variables,
            classes: FxHashMap::default(),
            print_buffer: String::new(),
            function_bodies: FxHashMap::default(),
            lambda_bodies: FxHashMap::default(),
            current_source: String::new(),
            body_source_stack: Vec::new(),
            operations_count: 0,
            execution_start: Instant::now(),
            decimal_prec: 28,
            memory_used_bytes: 0,
            call_depth: 0,
            method_frame_stack: Vec::new(),
            yield_stack: Vec::new(),
            generators: rustc_hash::FxHashMap::default(),
            active_generator_stack: Vec::new(),
            active_exception_stack: Vec::new(),
            lazy_cursors: FxHashMap::default(),
            next_cursor_id: 0,
            nonlocal_cells: FxHashMap::default(),
            next_nonlocal_cell_id: 0,
            frame_cell_owners: Vec::new(),
            tool_semaphore: Arc::new(Semaphore::new(config.max_concurrent_tools as usize)),
            config,
        }
    }

    /// Bump the call-depth counter for entry to a new user function /
    /// lambda frame. Returns `RecursionLimitExceeded` when depth would
    /// exceed `config.max_recursion_depth` — the counter is not bumped
    /// on that failure path, so the caller must only pair a successful
    /// `enter_call` with a matching `exit_call`.
    pub const fn enter_call(&mut self) -> Result<(), crate::error::InterpreterError> {
        if self.call_depth >= self.config.max_recursion_depth {
            return Err(crate::error::InterpreterError::RecursionLimitExceeded {
                limit: self.config.max_recursion_depth,
            });
        }
        self.call_depth = self.call_depth.saturating_add(1);
        Ok(())
    }

    /// Decrement the call-depth counter on exit from a user frame.
    pub const fn exit_call(&mut self) {
        self.call_depth = self.call_depth.saturating_sub(1);
    }

    /// Set a variable, tracking memory usage.
    /// Returns an error if the memory limit is exceeded.
    ///
    /// Write-through to nonlocal cells: if the current frame owns a
    /// cell for this name (i.e. some inner function captured it as
    /// nonlocal), the new value is also stored in `nonlocal_cells`
    /// so the inner sees the up-to-date value on its next call.
    /// CPython binds inner's nonlocal to an actual cell object
    /// shared with outer's local; this write-through is the
    /// flat-state equivalent.
    pub fn set_variable(
        &mut self,
        name: &str,
        value: Value,
    ) -> Result<(), crate::error::InterpreterError> {
        // Release old value's memory if overwriting
        if let Some(old) = self.variables.get(name) {
            let old_size = estimate_value_size(old);
            self.memory_used_bytes = self.memory_used_bytes.saturating_sub(old_size);
        }
        // Track new value's memory
        let new_size = estimate_value_size(&value);
        self.memory_used_bytes = self.memory_used_bytes.saturating_add(new_size);

        // Write-through to any inner's nonlocal cell that this frame owns.
        if let Some(owners) = self.frame_cell_owners.last() {
            if let Some(&cell_id) = owners.get(name) {
                self.nonlocal_cells
                    .entry(cell_id)
                    .or_default()
                    .insert(name.to_string(), value.clone());
            }
        }

        self.variables.insert(name.to_string(), value);
        // Check memory immediately for large values
        self.check_memory()
    }

    #[inline]
    pub fn get_variable(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    pub fn delete_variable(&mut self, name: &str) -> Result<(), crate::error::InterpreterError> {
        match self.variables.remove(name) {
            Some(old) => {
                self.memory_used_bytes =
                    self.memory_used_bytes.saturating_sub(estimate_value_size(&old));
                Ok(())
            }
            None => Err(crate::error::InterpreterError::name_not_defined(name)),
        }
    }

    /// Returns user-visible state keys (excludes internal keys starting with _).
    pub fn state_keys(&self) -> Vec<String> {
        self.variables.keys().filter(|k| !k.starts_with('_')).cloned().collect()
    }

    pub fn clear_print_buffer(&mut self) {
        self.print_buffer.clear();
    }

    pub fn append_print(&mut self, text: &str) -> Result<(), crate::error::InterpreterError> {
        let new_len = self.print_buffer.len() + text.len();
        // On 32-bit targets, max_stdout_bytes may exceed usize::MAX; saturate
        // to usize::MAX so the comparison stays meaningful (new_len is a usize
        // that cannot exceed usize::MAX regardless).
        let max_stdout = usize::try_from(self.config.max_stdout_bytes).unwrap_or(usize::MAX);
        if new_len > max_stdout {
            return Err(crate::error::InterpreterError::LimitExceeded(format!(
                "print output ({new_len} bytes) exceeds limit ({} bytes)",
                self.config.max_stdout_bytes
            )));
        }
        self.print_buffer.push_str(text);
        self.track_allocation(text.len())
    }

    pub const fn reset_operations(&mut self) {
        self.operations_count = 0;
    }

    pub fn increment_ops(&mut self) -> Result<(), crate::error::InterpreterError> {
        self.operations_count += 1;
        if self.operations_count >= self.config.max_operations {
            return Err(crate::error::InterpreterError::LimitExceeded(format!(
                "exceeded maximum of {} operations",
                self.config.max_operations
            )));
        }
        // Periodic checks every 100 ops (memory and timeout)
        if self.operations_count % 100 == 0 {
            self.check_memory()?;
            if let Some(max_time) = self.config.max_execution_time {
                if self.execution_start.elapsed() > max_time {
                    return Err(crate::error::InterpreterError::LimitExceeded(format!(
                        "execution time exceeded {max_time:?}"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Check if memory usage exceeds the configured limit.
    pub fn check_memory(&self) -> Result<(), crate::error::InterpreterError> {
        let max_memory = usize::try_from(self.config.max_memory_bytes).unwrap_or(usize::MAX);
        if self.memory_used_bytes > max_memory {
            return Err(crate::error::InterpreterError::LimitExceeded(format!(
                "memory usage ({} bytes) exceeds limit ({} bytes)",
                self.memory_used_bytes, self.config.max_memory_bytes
            )));
        }
        Ok(())
    }

    /// Track an allocation of `bytes` against the memory budget.
    /// Returns an error if the budget is exceeded.
    pub fn track_allocation(&mut self, bytes: usize) -> Result<(), crate::error::InterpreterError> {
        self.memory_used_bytes = self.memory_used_bytes.saturating_add(bytes);
        let max_memory = usize::try_from(self.config.max_memory_bytes).unwrap_or(usize::MAX);
        if self.memory_used_bytes > max_memory {
            Err(crate::error::InterpreterError::LimitExceeded(format!(
                "memory usage ({} bytes) exceeds limit ({} bytes)",
                self.memory_used_bytes, self.config.max_memory_bytes
            )))
        } else {
            Ok(())
        }
    }

    /// Release `bytes` from the memory budget (e.g., when a variable is deleted).
    pub const fn release_allocation(&mut self, bytes: usize) {
        self.memory_used_bytes = self.memory_used_bytes.saturating_sub(bytes);
    }
}

/// Per-`Value` enum slot footprint, including the discriminant. Set
/// from the boxed-exception layout (`size_of::<Value>() == 80` on the
/// supported 64-bit targets). Used by [`estimate_value_size`] to
/// account for the per-slot overhead in every container — without it,
/// a `[Value::Int(0); 1000]` reports 8_000 B (payload only) when its
/// actual heap footprint is dominated by the enum slots plus the `Vec`
/// header.
const VALUE_SLOT_BYTES: usize = 80;

/// `String` header footprint: pointer + length + capacity = 3 × `usize`
/// on a 64-bit target. `CompactString` matches the layout exactly —
/// SSO is in the header bits, not extra space — so the same constant
/// applies to both `String` and `CompactString` fields.
const STRING_HEADER_BYTES: usize = 24;

/// Per-entry hash bucket overhead in an `IndexMap`-style ordered map.
/// Approximate: each entry costs the key + value bytes plus a hash
/// slot (8 B) and a doubly-tracked-back-index slot (8 B). The bucket
/// table itself is sized by `IndexMap::capacity()`, which we don't
/// inspect; this per-entry constant rolls the amortised cost in.
const INDEXMAP_PER_ENTRY_BYTES: usize = 16;

/// Estimate the memory footprint of a `Value` in bytes. The result
/// includes per-`Value`-slot overhead, container headers, string
/// headers, and amortised hash-table per-entry costs so that
/// `Interpreter::accounted_bytes()` tracks within ~2× of true RSS for
/// typical workloads (vs the prior payload-only accounting that
/// under-reported by ~10×). The number gates `max_memory_bytes` in
/// the sandbox so honest accounting is load-bearing for resource
/// protection — under-reporting let snippets allocate ~10× their
/// configured budget before tripping the limit.
#[expect(
    clippy::match_same_arms,
    reason = "match arms are grouped by variant family (numerics, sequences, mappings, Track D types) for readability; merging same-body arms would scatter them across the table"
)]
pub fn estimate_value_size(value: &crate::value::Value) -> usize {
    use crate::value::Value;
    match value {
        Value::None | Value::NotImplemented | Value::Ellipsis => 0,
        Value::OperatorGetter(_) => 32,
        Value::Bool(_) => 1,
        // i64 and f64 are both 8 bytes.
        Value::Int(_) | Value::Float(_) => 8,
        // complex is a boxed pair of f64.
        Value::Complex(_) => 16,
        // Approximate limb storage for big integers.
        Value::BigInt(b) => 16 + (b.bits() as usize / 8).saturating_add(8),
        Value::String(s) => STRING_HEADER_BYTES + s.len(),
        Value::Bytes(b) => STRING_HEADER_BYTES + b.len(),
        // List is shared via Arc<Mutex<Vec>>; lock to walk under the
        // guard. Tuple/Set wrap plain Vec<Value> and walk directly.
        Value::List(items) => {
            let guard = items.lock();
            STRING_HEADER_BYTES
                + guard.len() * VALUE_SLOT_BYTES
                + guard.iter().map(estimate_value_size).sum::<usize>()
        }
        Value::Tuple(items) | Value::Set(items) | Value::Frozenset(items) => {
            STRING_HEADER_BYTES
                + items.len() * VALUE_SLOT_BYTES
                + items.iter().map(estimate_value_size).sum::<usize>()
        }
        Value::Dict(map) => {
            48 + map.len() * (INDEXMAP_PER_ENTRY_BYTES + VALUE_SLOT_BYTES)
                + map
                    .iter()
                    .map(|(k, v)| estimate_key_size(k) + estimate_value_size(v))
                    .sum::<usize>()
        }
        // `Function` and `Lambda` are `Arc<…>`-wrapped (see F2.5). Count each
        // Value::Function reference as a fixed pointer-plus-header weight,
        // **not** the recursive walk of the inner closure. Walking through
        // the Arc would reintroduce the O(2^N) over-count that motivated
        // the Arc switch: the same FunctionDef is reachable from many
        // closures, and the heap holds it once. The actual closure storage
        // is counted exactly once at construction time (eval_function_def),
        // through the closure entries it contains at that snapshot moment.
        Value::Function(_) | Value::Lambda(_) | Value::LazyProxy(_) => 64,
        Value::Range { .. } => 24,
        Value::Exception(e) => 32 + e.type_name.len() + e.message.len(),
        Value::ExceptionMethod { method, exception } => {
            32 + method.len() + exception.type_name.len() + exception.message.len()
        }
        // Type / class / module handles carry just a name.
        Value::Type(n) | Value::Class(n) | Value::Module(n) => 8 + n.len(),
        Value::ModuleFunction { module, name } => 16 + module.len() + name.len(),
        Value::Date(_) => 16,
        Value::ReMatch(m) => {
            16 + m.groups.iter().flatten().map(|g| g.text.len() + 16).sum::<usize>()
        }
        Value::RePattern(p) => 16 + p.len(),
        Value::Slice(_) => 32,
        Value::Instance(inst) => {
            16 + inst.class_name.len()
                + inst
                    .fields
                    .lock()
                    .iter()
                    .map(|(k, v)| k.len() + estimate_value_size(v))
                    .sum::<usize>()
        }
        Value::Super { defining_class, instance } => {
            16 + defining_class.len() + estimate_value_size(&Value::Instance((**instance).clone()))
        }
        Value::Counter(map) => {
            48 + map.len() * (INDEXMAP_PER_ENTRY_BYTES + VALUE_SLOT_BYTES)
                + map
                    .iter()
                    .map(|(k, v)| estimate_key_size(k) + estimate_value_size(v))
                    .sum::<usize>()
        }
        // 24 bytes for DateTime (matches Range); 12 for Time; 8 each
        // for TimeDelta (i64 micros) and TimeZone (i32 secs). Inlined
        // rather than merged with Range / Int arms because the
        // grouping reads as "Track D variants" — keeping them
        // adjacent helps readers.
        Value::DateTime { .. } => 24,
        Value::Time(_) => 12,
        Value::TimeDelta(_) | Value::TimeZone(_) => 8,
        Value::HashDigest { algo, bytes } => 16 + algo.len() + bytes.len(),
        Value::Deque { items, .. } => {
            STRING_HEADER_BYTES
                + items.len() * VALUE_SLOT_BYTES
                + items.iter().map(estimate_value_size).sum::<usize>()
        }
        Value::DefaultDict(data) => {
            48 + estimate_value_size(&data.factory)
                + data.items.len() * (INDEXMAP_PER_ENTRY_BYTES + VALUE_SLOT_BYTES)
                + data
                    .items
                    .iter()
                    .map(|(k, v)| estimate_key_size(k) + estimate_value_size(v))
                    .sum::<usize>()
        }
        Value::EnumMember { class_name, member_name, value, .. } => {
            32 + class_name.len() + member_name.len() + estimate_value_size(value)
        }
        // BigDecimal and BigRational allocate a digit vector on the
        // heap. We over-estimate at a fixed 48 (the largest practical
        // single-instance memory for typical inputs) rather than
        // recursing into the BigInt's digit count — the bound stays
        // tight for the common case (Decimal("3.14"), Fraction(3, 7))
        // and only loosens for very long decimals, which a Python
        // script under our sandbox is unlikely to construct.
        Value::Decimal(_) | Value::Fraction(_) => 48,
        // Bound-method snapshot: pointer-sized header + the captured
        // receiver's own footprint (which is itself bounded by the
        // memory limit since the receiver had to live in a variable).
        Value::BoundMethod { receiver, method } => {
            use crate::value::{BoundMethodReceiver, BoundMethodStep};
            let receiver_size = match receiver {
                BoundMethodReceiver::Snapshot(v) => estimate_value_size(v),
                BoundMethodReceiver::Place { root, steps } => {
                    root.len()
                        + steps
                            .iter()
                            .map(|s| match s {
                                BoundMethodStep::Index(v) => 8 + estimate_value_size(v),
                                BoundMethodStep::Attr(n) => 8 + n.len(),
                            })
                            .sum::<usize>()
                }
            };
            16 + method.len() + receiver_size
        }
        Value::BuiltinTypeMethod { type_name, method } => 16 + type_name.len() + method.len(),
        Value::BuiltinName(n) | Value::ToolName(n) | Value::ExceptionType(n) => 16 + n.len(),
        Value::UnboundClassMethod { class, method } => 16 + class.len() + method.len(),
        Value::Lazy { items, .. } => 24 + items.iter().map(estimate_value_size).sum::<usize>(),
        Value::Generator { .. } => 16,
        Value::Partial(data) => {
            16 + estimate_value_size(&data.func)
                + data.args.iter().map(estimate_value_size).sum::<usize>()
                + data.keywords.values().map(estimate_value_size).sum::<usize>()
        }
        Value::LruCache(data) => {
            16 + estimate_value_size(&data.func)
                + data.cache.lock().values().map(estimate_value_size).sum::<usize>()
        }
    }
}

/// Estimate the memory size of a `ValueKey` in bytes.
pub fn estimate_key_size(key: &crate::value::ValueKey) -> usize {
    use crate::value::ValueKey;
    match key {
        ValueKey::None | ValueKey::Ellipsis => 0,
        ValueKey::Bool(_) => 1,
        ValueKey::Int(_) | ValueKey::Float(_) => 8,
        ValueKey::Complex(..) => 16,
        ValueKey::BigInt(b) => 16 + (b.bits() as usize / 8).saturating_add(8),
        ValueKey::String(s) => s.len(),
        ValueKey::Tuple(items) | ValueKey::Frozenset(items) => {
            24 + items.iter().map(estimate_key_size).sum::<usize>()
        }
        ValueKey::Instance { value, .. } => 8 + estimate_value_size(value),
    }
}
