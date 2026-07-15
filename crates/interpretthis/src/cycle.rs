// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cycle guards for recursive `Value` traversals.
//!
//! A container can reference itself (`lst.append(lst)`, `d['self'] = d`), so any
//! function that walks a value's nested elements — `repr`, `to_json`, equality —
//! would recurse forever without protection. Every cycle passes through at least
//! one Arc-backed mutable container (list/dict/instance fields/array), so a
//! thread-local set of the pointers *currently on the traversal stack* (keyed by
//! `Arc::as_ptr`) breaks the recursion when one is re-entered. Each traversal
//! keeps its own set so they don't interfere, and an RAII guard pops the pointer
//! on the way back up. This mirrors CPython's `Py_ReprEnter`/`Py_ReprLeave`
//! (repr) and the `Py_EnterRecursiveCall` depth limit (equality).
//!
//! `estimate_value_size` (memory accounting) does not use these sets: it holds
//! the container lock while walking, so a re-entrant `lock` there would deadlock
//! before recursing — it uses `try_lock` and skips a locked container instead.

use std::cell::{Cell, RefCell};

use rustc_hash::FxHashSet;

macro_rules! active_set {
    ($tl:ident, $enter:ident, $guard:ident, $doc:literal) => {
        thread_local! {
            static $tl: RefCell<FxHashSet<usize>> = RefCell::new(FxHashSet::default());
        }
        #[doc = $doc]
        pub(crate) fn $enter(ptr: usize) -> Option<$guard> {
            // `if`/`else`, not `then_some`: `then_some` constructs the guard
            // eagerly even on the `false` (already-present) branch, and dropping
            // that discarded guard would re-borrow the RefCell that `insert` is
            // still holding. Here the guard is built only after the borrow ends.
            let newly = $tl.with(|s| s.borrow_mut().insert(ptr));
            newly.then(|| $guard(ptr))
        }
        pub(crate) struct $guard(usize);
        impl Drop for $guard {
            fn drop(&mut self) {
                $tl.with(|s| {
                    s.borrow_mut().remove(&self.0);
                });
            }
        }
    };
}

active_set!(
    REPR_ACTIVE,
    repr_enter,
    ReprGuard,
    "Enter a container during `repr`/`str`. `None` means it is already being formatted (a cycle) — the caller emits the ellipsis form (`[...]` / `{...}`)."
);
active_set!(
    JSON_ACTIVE,
    json_enter,
    JsonGuard,
    "Enter a container during JSON serialization. `None` means it is already being serialized (a cycle) — the caller raises `Circular reference detected`."
);

thread_local! {
    static EQ_DEPTH: Cell<u32> = const { Cell::new(0) };
    static EQ_OVERFLOW: Cell<bool> = const { Cell::new(false) };
}

/// The comparison-recursion depth limit (mirrors the default
/// `max_recursion_depth`, CPython's 1000).
pub(crate) const EQ_RECURSION_LIMIT: u32 = 1000;

/// Enter a level of the equality/ordering recursion. `None` means the depth
/// limit is reached: the sync comparator (which returns `bool`, not `Result`)
/// unwinds returning "not equal", and it *sets a thread-local overflow flag* so
/// the enclosing comparison operator — which does return `Result` — can turn it
/// into `RecursionError`, matching CPython's `Py_EnterRecursiveCall`. Returns an
/// RAII guard that decrements on drop.
pub(crate) fn eq_depth_enter() -> Option<EqDepthGuard> {
    EQ_DEPTH.with(|d| {
        let cur = d.get();
        if cur == 0 {
            // Fresh top-level comparison: clear any stale flag from a previous
            // one so a non-`Result` caller that swallowed the overflow can't
            // poison this comparison.
            EQ_OVERFLOW.with(|o| o.set(false));
        }
        if cur >= EQ_RECURSION_LIMIT {
            EQ_OVERFLOW.with(|o| o.set(true));
            None
        } else {
            d.set(cur + 1);
            Some(EqDepthGuard(()))
        }
    })
}

/// Read and clear the equality-recursion overflow flag. A comparison /
/// membership operator calls this right after computing its result: `true`
/// means the structural walk hit [`EQ_RECURSION_LIMIT`] (a cyclic or
/// pathologically deep comparison) and the operator should raise
/// `RecursionError` instead of returning the truncated result.
pub(crate) fn take_eq_overflow() -> bool {
    EQ_OVERFLOW.with(std::cell::Cell::take)
}

pub(crate) struct EqDepthGuard(());
impl Drop for EqDepthGuard {
    fn drop(&mut self) {
        EQ_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}
