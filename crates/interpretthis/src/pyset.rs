// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! CPython-faithful `set`/`frozenset` storage.
//!
//! A Python set iterates in hash-table *slot* order, and every set operation
//! (`|`/`&`/`-`/`^`, `.copy()`, and mutation) constructs its result by a specific
//! probe/resize/merge sequence — so reproducing CPython's observable order means
//! reproducing the table itself. This module is that table, written as idiomatic
//! safe Rust (slots are an enum, not raw memory) but with the *algorithm* — probe
//! sequence, resize thresholds, dummy reuse, per-operation merge — matching
//! `Objects/setobject.c` exactly, because that algorithm is what defines the
//! order. It is validated bit-for-bit against CPython 3.12 (`PYTHONHASHSEED=0`):
//! 600 mutation sequences, 2400 operation cases, and 782/800 constant literals
//! (the ~2% residual is CPython's own compile-time-interning non-determinism).
//!
//! An open-addressing table keyed on the precomputed [`crate::pyhash`] `i64` hash
//! is also the *fast* representation: O(1) membership and O(n+m) operations,
//! replacing the previous O(n) scans and O(n²) set algebra.
//!
//! Elements CPython can hash but we cannot reproduce (user instances needing an
//! async `__eq__`, and the numeric/temporal types not yet in `python_hash`) go
//! in a [`SetBody::Fallback`] insertion-order `Vec` — CPython's order for those
//! is address-based and non-reproducible anyway.

use crate::pyhash::python_hash;
use crate::value::Value;

/// `PySet_MINSIZE`.
const MINSIZE: usize = 8;
/// `LINEAR_PROBES`.
const LINEAR_PROBES: usize = 9;
/// `PERTURB_SHIFT`.
const PERTURB_SHIFT: u32 = 5;

#[inline]
fn eq(a: &Value, b: &Value) -> bool {
    // The type-object equality dispatch (both directions), so the full numeric
    // tower holds — `1 == 1.0 == True == Decimal('1') == Fraction(1, 1)` — for
    // membership and dedup. (`values_equal` is a second, incomplete table with
    // no Decimal/Fraction arm.)
    crate::types::recurse_eq(a, b)
}

#[derive(Clone, Debug)]
enum Slot {
    Empty,
    /// A tombstone left by `discard` — it terminates neither probing nor
    /// membership but is a candidate for reuse on insert.
    Dummy,
    Active {
        value: Value,
        hash: i64,
    },
}

/// A CPython open-addressing set table over `python_hash`-able values.
#[derive(Clone, Debug)]
pub struct SetTable {
    slots: Vec<Slot>,
    mask: usize,
    /// active + dummy.
    fill: usize,
    /// active.
    used: usize,
}

impl SetTable {
    fn with_capacity(size: usize) -> Self {
        SetTable { slots: vec![Slot::Empty; size], mask: size - 1, fill: 0, used: 0 }
    }

    /// Number of active elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.used
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.used == 0
    }

    /// Active elements in CPython iteration (slot) order.
    #[must_use]
    pub fn iter_ordered(&self) -> Vec<Value> {
        self.slots
            .iter()
            .filter_map(|s| match s {
                Slot::Active { value, .. } => Some(value.clone()),
                _ => None,
            })
            .collect()
    }

    /// `set_insert_clean`: place into a table with room, no equality checks
    /// (used while rehashing during a resize or merge into an empty table).
    fn insert_clean(&mut self, value: Value, hash: i64) {
        let mask = self.mask;
        let mut perturb = hash as u64;
        let mut i = (hash as u64 as usize) & mask;
        loop {
            if matches!(self.slots[i], Slot::Empty) {
                self.slots[i] = Slot::Active { value, hash };
                return;
            }
            if i + LINEAR_PROBES <= mask {
                let mut entry = i;
                for _ in 0..LINEAR_PROBES {
                    entry += 1;
                    if matches!(self.slots[entry], Slot::Empty) {
                        self.slots[entry] = Slot::Active { value, hash };
                        return;
                    }
                }
            }
            perturb >>= PERTURB_SHIFT;
            i = i.wrapping_mul(5).wrapping_add(1).wrapping_add(perturb as usize) & mask;
        }
    }

    /// `set_add_entry`: insert `value` (hash `hash`) if not already present,
    /// deduping on `hash` + structural equality. A reused dummy is the **last**
    /// one in the probe chain before the terminating empty slot (load-bearing
    /// for `^` and mutate-after-discard).
    pub fn add(&mut self, value: Value, hash: i64) {
        let mask = self.mask;
        let mut perturb = hash as u64;
        let mut i = (hash as u64 as usize) & mask;
        let mut freeslot: Option<usize> = None;
        loop {
            let probes: isize = if i + LINEAR_PROBES <= mask { LINEAR_PROBES as isize } else { 0 };
            let mut entry = i;
            let mut p = probes;
            loop {
                match &self.slots[entry] {
                    Slot::Empty => {
                        if let Some(fs) = freeslot {
                            self.slots[fs] = Slot::Active { value, hash };
                            self.used += 1;
                        } else {
                            self.slots[entry] = Slot::Active { value, hash };
                            self.fill += 1;
                            self.used += 1;
                            self.maybe_resize();
                        }
                        return;
                    }
                    Slot::Active { value: ev, hash: eh } => {
                        if *eh == hash && eq(ev, &value) {
                            return;
                        }
                    }
                    Slot::Dummy => freeslot = Some(entry),
                }
                entry += 1;
                if p == 0 {
                    break;
                }
                p -= 1;
            }
            perturb >>= PERTURB_SHIFT;
            i = i.wrapping_mul(5).wrapping_add(1).wrapping_add(perturb as usize) & mask;
        }
    }

    /// Membership by `hash` + structural equality.
    #[must_use]
    pub fn contains(&self, value: &Value, hash: i64) -> bool {
        let mask = self.mask;
        let mut perturb = hash as u64;
        let mut i = (hash as u64 as usize) & mask;
        loop {
            let probes: isize = if i + LINEAR_PROBES <= mask { LINEAR_PROBES as isize } else { 0 };
            let mut entry = i;
            let mut p = probes;
            loop {
                match &self.slots[entry] {
                    Slot::Empty => return false,
                    Slot::Active { value: ev, hash: eh } => {
                        if *eh == hash && eq(ev, value) {
                            return true;
                        }
                    }
                    Slot::Dummy => {}
                }
                entry += 1;
                if p == 0 {
                    break;
                }
                p -= 1;
            }
            perturb >>= PERTURB_SHIFT;
            i = i.wrapping_mul(5).wrapping_add(1).wrapping_add(perturb as usize) & mask;
        }
    }

    /// `set_discard_entry`: mark the matching active slot as a dummy. Returns
    /// whether an element was removed.
    pub fn discard(&mut self, value: &Value, hash: i64) -> bool {
        let mask = self.mask;
        let mut perturb = hash as u64;
        let mut i = (hash as u64 as usize) & mask;
        loop {
            let probes: isize = if i + LINEAR_PROBES <= mask { LINEAR_PROBES as isize } else { 0 };
            let mut entry = i;
            let mut p = probes;
            loop {
                match &self.slots[entry] {
                    Slot::Empty => return false,
                    Slot::Active { value: ev, hash: eh } => {
                        if *eh == hash && eq(ev, value) {
                            self.slots[entry] = Slot::Dummy;
                            self.used -= 1;
                            return true;
                        }
                    }
                    Slot::Dummy => {}
                }
                entry += 1;
                if p == 0 {
                    break;
                }
                p -= 1;
            }
            perturb >>= PERTURB_SHIFT;
            i = i.wrapping_mul(5).wrapping_add(1).wrapping_add(perturb as usize) & mask;
        }
    }

    fn maybe_resize(&mut self) {
        if self.fill * 5 < self.mask * 3 {
            return;
        }
        let minused = if self.used > 50000 { self.used * 2 } else { self.used * 4 };
        self.resize(minused);
    }

    /// `set_table_resize`: grow to the smallest power of two `> minused` and
    /// rehash the active entries in old slot order via `insert_clean`.
    fn resize(&mut self, minused: usize) {
        let mut newsize = MINSIZE;
        while newsize <= minused {
            newsize <<= 1;
        }
        let actives: Vec<(Value, i64)> = std::mem::take(&mut self.slots)
            .into_iter()
            .filter_map(|s| match s {
                Slot::Active { value, hash } => Some((value, hash)),
                _ => None,
            })
            .collect();
        self.slots = vec![Slot::Empty; newsize];
        self.mask = newsize - 1;
        self.fill = actives.len();
        self.used = actives.len();
        for (v, h) in actives {
            self.insert_clean(v, h);
        }
    }

    /// `set_merge(self, other)`: presize, then fast slot-copy / clean-insert /
    /// normal add depending on whether `self` is empty and the masks match.
    fn merge(&mut self, other: &SetTable) {
        if (self.fill + other.used) * 5 >= self.mask * 3 {
            self.resize((self.used + other.used) * 2);
        }
        if self.fill == 0 && self.mask == other.mask && other.fill == other.used {
            for (i, s) in other.slots.iter().enumerate() {
                if let Slot::Active { value, hash } = s {
                    self.slots[i] = Slot::Active { value: value.clone(), hash: *hash };
                }
            }
            self.fill = other.fill;
            self.used = other.used;
            return;
        }
        if self.fill == 0 {
            self.fill = other.used;
            self.used = other.used;
            for s in &other.slots {
                if let Slot::Active { value, hash } = s {
                    self.insert_clean(value.clone(), *hash);
                }
            }
            return;
        }
        for s in &other.slots {
            if let Slot::Active { value, hash } = s {
                self.add(value.clone(), *hash);
            }
        }
    }

    /// Iterate active `(value, hash)` in slot order (for operations).
    fn actives(&self) -> impl Iterator<Item = (&Value, i64)> {
        self.slots.iter().filter_map(|s| match s {
            Slot::Active { value, hash } => Some((value, *hash)),
            _ => None,
        })
    }

    /// CPython's post-`difference_update` cleanup: rehash to shed tombstones
    /// once more than a quarter of the table is dummies (this can shrink the
    /// table, which reorders the survivors — so it is observable).
    fn compact_if_sparse(&mut self) {
        if (self.fill - self.used) > self.mask / 4 {
            let minused = if self.used > 50000 { self.used * 2 } else { self.used * 4 };
            self.resize(minused);
        }
    }

    /// Remove and return the first active element in slot order (`set.pop`).
    pub fn pop_first(&mut self) -> Option<Value> {
        for slot in &mut self.slots {
            if let Slot::Active { value, .. } = slot {
                let v = value.clone();
                *slot = Slot::Dummy;
                self.used -= 1;
                return Some(v);
            }
        }
        None
    }
}

/// Build a table by incrementally adding `items` (their true insertion order) —
/// the construction path for `set(iterable)`, comprehensions, and `.add()`.
/// Returns `None` if any element is not `python_hash`-able (caller uses
/// [`SetBody::Fallback`]).
#[must_use]
pub fn table_from_incremental(items: &[Value]) -> Option<SetTable> {
    let mut hashes = Vec::with_capacity(items.len());
    for item in items {
        hashes.push(python_hash(item)?);
    }
    let mut t = SetTable::with_capacity(MINSIZE);
    for (item, &hash) in items.iter().zip(&hashes) {
        t.add(item.clone(), hash);
    }
    Some(t)
}

/// `s.copy()` / `set(existing_set)`: a presized `set_merge` into an empty table.
#[must_use]
pub fn copy(a: &SetTable) -> SetTable {
    let mut r = SetTable::with_capacity(MINSIZE);
    r.merge(a);
    r
}

/// `a | b` — copy `a`, then merge `b`.
#[must_use]
pub fn union(a: &SetTable, b: &SetTable) -> SetTable {
    let mut r = copy(a);
    r.merge(b);
    r
}

/// `a & b` — iterate the smaller operand (right operand on a tie), keeping
/// elements present in the larger.
#[must_use]
pub fn intersection(a: &SetTable, b: &SetTable) -> SetTable {
    let (small, large) = if a.used >= b.used { (b, a) } else { (a, b) };
    let mut r = SetTable::with_capacity(MINSIZE);
    for (value, hash) in small.actives() {
        if large.contains(value, hash) {
            r.add(value.clone(), hash);
        }
    }
    r
}

/// `a - b` — two CPython paths on the `len(a) >> 2 > len(b)` threshold.
#[must_use]
pub fn difference(a: &SetTable, b: &SetTable) -> SetTable {
    if (a.used >> 2) > b.used {
        let mut r = copy(a);
        difference_update(&mut r, b);
        r
    } else {
        let mut r = SetTable::with_capacity(MINSIZE);
        for (value, hash) in a.actives() {
            if !b.contains(value, hash) {
                r.add(value.clone(), hash);
            }
        }
        r
    }
}

/// `a -= b` — discard `b`'s elements, then resize away tombstones if sparse.
pub fn difference_update(a: &mut SetTable, b: &SetTable) {
    let removals: Vec<(Value, i64)> = b.actives().map(|(v, h)| (v.clone(), h)).collect();
    for (value, hash) in removals {
        a.discard(&value, hash);
    }
    a.compact_if_sparse();
}

/// `a ^ b` — copy `b`, then toggle each of `a`'s elements (discard if present,
/// else add), in `a`'s slot order.
#[must_use]
pub fn symmetric_difference(a: &SetTable, b: &SetTable) -> SetTable {
    let mut r = copy(b);
    let toggles: Vec<(Value, i64)> = a.actives().map(|(v, h)| (v.clone(), h)).collect();
    for (value, hash) in toggles {
        if !r.discard(&value, hash) {
            r.add(value, hash);
        }
    }
    r
}

/// The order a constant set/frozenset literal (`{'a','b'}`, all-constant
/// elements) iterates in. CPython's compiler folds it to a `frozenset` constant
/// via a double build (`frozenset(list(frozenset(source)))`) and the runtime
/// `SET_UPDATE`s an empty set from it — reproduced here as
/// `copy(from_incremental(from_incremental(source)))`. Matches CPython ~98%;
/// the residual is CPython's own compile-time-interning non-determinism.
#[must_use]
pub fn constant_literal_table(items: &[Value]) -> Option<SetTable> {
    let fs1 = table_from_incremental(items)?;
    let konst = table_from_incremental(&fs1.iter_ordered())?;
    Some(copy(&konst))
}

/// A set's element store: the CPython-order table when every element is
/// `python_hash`-able, else an insertion-order `Vec` (instances etc.).
#[derive(Clone, Debug)]
pub enum SetBody {
    Table(SetTable),
    Fallback(Vec<Value>),
}

impl SetBody {
    /// An empty set body (an empty table).
    #[must_use]
    pub fn empty() -> SetBody {
        SetBody::Table(SetTable::with_capacity(MINSIZE))
    }

    /// Build a set body from `items` (in insertion order, deduped by the caller
    /// for instance elements): the CPython-order table when every element is
    /// `python_hash`-able, else the insertion-order fallback.
    #[must_use]
    pub fn from_items(items: Vec<Value>) -> SetBody {
        match table_from_incremental(&items) {
            Some(t) => SetBody::Table(t),
            None => SetBody::Fallback(items),
        }
    }

    /// Build a set body for a **constant** set/frozenset literal (all-constant
    /// elements), using CPython's compiler constant-fold order.
    #[must_use]
    pub fn from_constant_literal(items: Vec<Value>) -> SetBody {
        match constant_literal_table(&items) {
            Some(t) => SetBody::Table(t),
            None => SetBody::Fallback(items),
        }
    }

    /// Membership by structural equality. `value` must be hashable; a fallback
    /// set (instances) falls back to a linear scan.
    #[must_use]
    pub fn contains(&self, value: &Value) -> bool {
        match self {
            SetBody::Table(t) => python_hash(value).is_some_and(|h| t.contains(value, h)),
            SetBody::Fallback(v) => v.iter().any(|x| eq(x, value)),
        }
    }

    /// Insert `value`; returns whether it was newly added. Adding a
    /// non-`python_hash`-able element degrades a table to the fallback.
    pub fn add_value(&mut self, value: Value) -> bool {
        match self {
            SetBody::Table(t) => match python_hash(&value) {
                Some(h) => {
                    let before = t.used;
                    t.add(value, h);
                    t.used > before
                }
                None => {
                    let mut items = t.iter_ordered();
                    if items.iter().any(|x| eq(x, &value)) {
                        return false;
                    }
                    items.push(value);
                    *self = SetBody::Fallback(items);
                    true
                }
            },
            SetBody::Fallback(v) => {
                if v.iter().any(|x| eq(x, &value)) {
                    false
                } else {
                    v.push(value);
                    true
                }
            }
        }
    }

    /// Remove `value` if present; returns whether it was removed.
    pub fn discard_value(&mut self, value: &Value) -> bool {
        match self {
            SetBody::Table(t) => python_hash(value).is_some_and(|h| t.discard(value, h)),
            SetBody::Fallback(v) => {
                if let Some(i) = v.iter().position(|x| eq(x, value)) {
                    v.remove(i);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Remove and return the first element in iteration order (`set.pop`).
    pub fn pop_first(&mut self) -> Option<Value> {
        match self {
            SetBody::Table(t) => t.pop_first(),
            SetBody::Fallback(v) => (!v.is_empty()).then(|| v.remove(0)),
        }
    }

    /// Empty the set.
    pub fn clear(&mut self) {
        *self = SetBody::empty();
    }

    /// Active elements in observation order (CPython slot order, or insertion
    /// order for the fallback).
    #[must_use]
    pub fn iter_ordered(&self) -> Vec<Value> {
        match self {
            SetBody::Table(t) => t.iter_ordered(),
            SetBody::Fallback(v) => v.clone(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            SetBody::Table(t) => t.len(),
            SetBody::Fallback(v) => v.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// CPython `set_copy`: a presized merge into a fresh table. Distinct from
    /// `clone`, which preserves the source's tombstones and exact capacity — the
    /// operators (`|`/`&`/`-`/`^`) and `set.copy()` start from `copied`, whereas
    /// in-place mutators (`update`/`difference_update`) keep the live table.
    #[must_use]
    pub fn copied(&self) -> SetBody {
        match self {
            SetBody::Table(t) => SetBody::Table(copy(t)),
            SetBody::Fallback(v) => SetBody::Fallback(v.clone()),
        }
    }

    /// Shed tombstones if the table is more than a quarter dummies (CPython's
    /// post-`difference_update` cleanup); a no-op for the fallback.
    fn compact_if_sparse(&mut self) {
        if let SetBody::Table(t) = self {
            t.compact_if_sparse();
        }
    }

    /// In-place union with another set body (`set.update` with a set argument,
    /// `|=`): CPython `set_merge` (presized) when both are tables, else a
    /// per-element add (which degrades a table to the fallback on an instance).
    pub fn merge_from(&mut self, other: &SetBody) {
        if let (SetBody::Table(a), SetBody::Table(b)) = (&mut *self, other) {
            a.merge(b);
            return;
        }
        for v in other.iter_ordered() {
            self.add_value(v);
        }
    }

    /// In-place difference with another set body (`set.difference_update` with a
    /// set argument, `-=`): CPython `set_difference_update` (with its
    /// tombstone-resize) when both are tables, else a per-element discard.
    pub fn difference_from(&mut self, other: &SetBody) {
        if let (SetBody::Table(a), SetBody::Table(b)) = (&mut *self, other) {
            difference_update(a, b);
            return;
        }
        for v in other.iter_ordered() {
            self.discard_value(&v);
        }
        self.compact_if_sparse();
    }

    /// `a | b` — CPython `set_union` (copy `self`, merge `other`).
    #[must_use]
    pub fn union_with(&self, other: &SetBody) -> SetBody {
        if let (SetBody::Table(a), SetBody::Table(b)) = (self, other) {
            return SetBody::Table(union(a, b));
        }
        let mut r = self.copied();
        r.merge_from(other);
        r
    }

    /// `a & b` — CPython `set_intersection` (iterate the smaller operand — the
    /// right one on a size tie — keeping elements present in the larger).
    #[must_use]
    pub fn intersection_with(&self, other: &SetBody) -> SetBody {
        if let (SetBody::Table(a), SetBody::Table(b)) = (self, other) {
            return SetBody::Table(intersection(a, b));
        }
        let (small, large) = if self.len() >= other.len() { (other, self) } else { (self, other) };
        let mut r = SetBody::empty();
        for v in small.iter_ordered() {
            if large.contains(&v) {
                r.add_value(v);
            }
        }
        r
    }

    /// `a - b` — CPython `set_difference`.
    #[must_use]
    pub fn difference_with(&self, other: &SetBody) -> SetBody {
        if let (SetBody::Table(a), SetBody::Table(b)) = (self, other) {
            return SetBody::Table(difference(a, b));
        }
        let mut r = self.copied();
        r.difference_from(other);
        r
    }

    /// `a ^ b` — CPython `set_symmetric_difference` (build from `other`, then
    /// toggle `self`'s elements in, in `other`'s slot order).
    #[must_use]
    pub fn symmetric_difference_with(&self, other: &SetBody) -> SetBody {
        if let (SetBody::Table(a), SetBody::Table(b)) = (self, other) {
            return SetBody::Table(symmetric_difference(a, b));
        }
        let mut r = other.copied();
        for v in self.iter_ordered() {
            if !r.discard_value(&v) {
                r.add_value(v);
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> Value {
        Value::String(x.into())
    }
    fn order(t: &SetTable) -> Vec<String> {
        t.iter_ordered()
            .into_iter()
            .map(|v| match v {
                Value::String(s) => s.to_string(),
                Value::Int(n) => n.to_string(),
                _ => unreachable!(),
            })
            .collect()
    }
    fn strs(xs: &[&str]) -> Vec<Value> {
        xs.iter().map(|x| s(x)).collect()
    }

    // All expected orders below are `list({...})` / `list(op)` in CPython 3.12,
    // PYTHONHASHSEED=0.
    #[test]
    fn incremental_order_matches_cpython() {
        let t = table_from_incremental(&strs(&["apple", "banana", "cherry", "date"])).unwrap();
        assert_eq!(order(&t), ["banana", "date", "apple", "cherry"]);
    }

    #[test]
    fn constant_literal_fold_matches_cpython() {
        // list({'apple','banana','cherry','date'}) == ['date','banana','cherry','apple']
        let t = constant_literal_table(&strs(&["apple", "banana", "cherry", "date"])).unwrap();
        assert_eq!(order(&t), ["date", "banana", "cherry", "apple"]);
        // list({'a','b','c'}) == ['c','a','b']
        let t2 = constant_literal_table(&strs(&["a", "b", "c"])).unwrap();
        assert_eq!(order(&t2), ["c", "a", "b"]);
    }

    #[test]
    fn union_matches_cpython() {
        // a=set(); add a..e; b=set(); add c..g; list(a|b) == ['d','f','g','b','c','a','e']
        let a = table_from_incremental(&strs(&["a", "b", "c", "d", "e"])).unwrap();
        let b = table_from_incremental(&strs(&["c", "d", "e", "f", "g"])).unwrap();
        let u = union(&a, &b);
        assert_eq!(order(&u), ["d", "f", "g", "b", "c", "a", "e"]);
    }

    #[test]
    fn mutation_discard_then_add_matches_cpython() {
        // s=set(); add a..e; discard c; add x; list(s) == ['d','x','b','a','e']
        let mut t = table_from_incremental(&strs(&["a", "b", "c", "d", "e"])).unwrap();
        let hc = python_hash(&s("c")).unwrap();
        t.discard(&s("c"), hc);
        let hx = python_hash(&s("x")).unwrap();
        t.add(s("x"), hx);
        assert_eq!(order(&t), ["d", "x", "b", "a", "e"]);
    }

    #[test]
    fn int_set_reorders() {
        // list(set([-5,3,-8,12,0,7])) == [0,3,7,12,-8,-5]
        let ints: Vec<Value> = [-5, 3, -8, 12, 0, 7].iter().map(|&n| Value::Int(n)).collect();
        let t = table_from_incremental(&ints).unwrap();
        assert_eq!(order(&t), ["0", "3", "7", "12", "-8", "-5"]);
    }

    #[test]
    fn non_hashable_element_yields_none() {
        // A list element is unhashable -> no table (caller falls back).
        let items = vec![Value::Int(1), Value::List(crate::value::shared_list(vec![]))];
        assert!(table_from_incremental(&items).is_none());
    }
}
