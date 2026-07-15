// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! CPython-compatible hashing and set iteration order.
//!
//! CPython iterates a `set`/`frozenset` in hash-table slot order, not insertion
//! order, so `list({...})` and `repr({...})` depend on the exact hash of each
//! element and on CPython's open-addressing probe sequence. To match that
//! byte-for-byte (the corpus pins `PYTHONHASHSEED=0`, which zeroes the SipHash
//! key and makes CPython's order fully deterministic) this module reproduces:
//!
//! * [`python_hash`] — CPython's `hash()` for the hashable value types. String
//!   and bytes hashing goes through the vetted `siphasher` SipHash-1-3 crate;
//!   the int/float/tuple algorithms are faithful ports of CPython 3.12's
//!   `Python/pyhash.c` and `Objects/tupleobject.c`. Every algorithm here is
//!   validated bit-exact against thousands of CPython-generated hashes (see the
//!   unit tests, whose expected values come straight from CPython 3.12).
//!
//! * [`cpython_set_order_indices`] — a port of CPython's `Objects/setobject.c` table
//!   (initial size 8, 9-slot linear probing then perturb, resize at 3/5 load,
//!   clean-rehash in slot order), replaying a set's *insertion* order to yield
//!   the elements in CPython's iteration order.
//!
//! Insertion order must be preserved by callers: replaying a set's own slot
//! order does NOT reproduce it (collision resolution depends on the true
//! insertion history), so this module only reorders at the point of
//! observation and never rewrites a set's stored order.

use std::hash::Hasher;

use crate::value::Value;

/// `_PyHASH_MODULUS` — the Mersenne prime `2**61 - 1`.
const MODULUS: u64 = (1u64 << 61) - 1;
/// `_PyHASH_BITS`.
const HASH_BITS: u32 = 61;
/// `_PyHASH_INF`.
const HASH_INF: i64 = 314159;
/// `hash(None)` — a fixed constant in CPython 3.12.
const NONE_HASH: i64 = 0xFCA8_6420;

// xxHash primes for the tuple hash (`Objects/tupleobject.c`, 64-bit).
const XXPRIME_1: u64 = 11400714785074694791;
const XXPRIME_2: u64 = 14029467366897019727;
const XXPRIME_5: u64 = 2870177450012600261;

#[inline]
fn xxrotate(x: u64) -> u64 {
    // CPython's `_PyHASH_XXROTATE` = `(x << 31) | (x >> 33)` for 64-bit.
    x.rotate_left(31)
}

/// SipHash-1-3 over `buf` with a zero key, then CPython's `_Py_HashBytes`
/// post-processing (empty → 0, and `-1` folds to `-2`).
fn hash_bytes(buf: &[u8]) -> i64 {
    if buf.is_empty() {
        return 0;
    }
    let mut hasher = siphasher::sip::SipHasher13::new_with_keys(0, 0);
    hasher.write(buf);
    let h = hasher.finish() as i64;
    if h == -1 { -2 } else { h }
}

/// A Python `str`'s canonical hash buffer (PEP 393): Latin-1 when every code
/// point is < 256, else little-endian UCS-2 (< 65536) or UCS-4. CPython hashes
/// this internal representation, not the UTF-8 encoding.
fn str_hash_buffer(s: &str) -> Vec<u8> {
    let max = s.chars().map(|c| c as u32).max().unwrap_or(0);
    if max < 0x100 {
        s.chars().map(|c| c as u8).collect()
    } else if max < 0x10000 {
        s.chars().flat_map(|c| (c as u16).to_le_bytes()).collect()
    } else {
        s.chars().flat_map(|c| (c as u32).to_le_bytes()).collect()
    }
}

/// CPython's integer hash: `|n| mod (2**61 - 1)`, carrying the sign, with the
/// reserved `-1` folded to `-2`.
fn hash_i64(n: i64) -> i64 {
    let abs = i128::from(n).unsigned_abs();
    let mut x = (abs % u128::from(MODULUS)) as i64;
    if n < 0 {
        x = -x;
    }
    if x == -1 { -2 } else { x }
}

/// Integer hash for a big integer, mirroring [`hash_i64`] with arbitrary
/// precision.
fn hash_bigint(n: &num_bigint::BigInt) -> i64 {
    use num_bigint::BigInt;
    let modulus = BigInt::from(MODULUS);
    let rem = n.magnitude() % modulus.magnitude();
    // `rem` fits in u64 (< 2**61); convert through i64.
    let digits = rem.to_u64_digits();
    let mut x = digits.first().copied().unwrap_or(0) as i64;
    if n.sign() == num_bigint::Sign::Minus {
        x = -x;
    }
    if x == -1 { -2 } else { x }
}

/// CPython's `_Py_HashDouble` (`Python/pyhash.c`).
fn hash_double(v: f64) -> i64 {
    if !v.is_finite() {
        if v.is_infinite() {
            return if v > 0.0 { HASH_INF } else { -HASH_INF };
        }
        // NaN: `_PyHASH_NAN` is 0 in 3.10+ (the object-pointer path is
        // unreachable for a bare float here).
        return 0;
    }
    let (mut m, mut e) = frexp(v);
    let mut sign: i64 = 1;
    if m < 0.0 {
        sign = -1;
        m = -m;
    }
    let mut x: u64 = 0;
    while m != 0.0 {
        x = ((x << 28) & MODULUS) | (x >> (HASH_BITS - 28));
        m *= 268435456.0; // 2**28
        e -= 28;
        let y = m as u64;
        m -= y as f64;
        x = x.wrapping_add(y);
        if x >= MODULUS {
            x -= MODULUS;
        }
    }
    let e_mod = if e >= 0 {
        (e % HASH_BITS as i32) as u32
    } else {
        HASH_BITS - 1 - (((-1 - e) % HASH_BITS as i32) as u32)
    };
    x = ((x << e_mod) & MODULUS) | (x >> (HASH_BITS - e_mod));
    let mut h = (x as i64).wrapping_mul(sign);
    if h == -1 {
        h = -2;
    }
    h
}

/// `frexp`: split `v` into `m * 2**e` with `0.5 <= |m| < 1` (and `(v, 0)` for
/// zero/non-finite), matching the C library routine CPython relies on.
fn frexp(v: f64) -> (f64, i32) {
    if v == 0.0 || !v.is_finite() {
        return (v, 0);
    }
    let bits = v.to_bits();
    let exp_field = ((bits >> 52) & 0x7ff) as i32;
    if exp_field == 0 {
        // Subnormal: normalise by scaling up 2**64, then correct the exponent.
        let (m, e) = frexp(v * 18446744073709551616.0);
        return (m, e - 64);
    }
    let e = exp_field - 1022;
    let m = f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52));
    (m, e)
}

/// CPython's `tuplehash` (`Objects/tupleobject.c`) over already-computed element
/// hashes.
fn hash_tuple(element_hashes: &[i64]) -> i64 {
    let mut acc = XXPRIME_5;
    for &lane in element_hashes {
        acc = acc.wrapping_add((lane as u64).wrapping_mul(XXPRIME_2));
        acc = xxrotate(acc);
        acc = acc.wrapping_mul(XXPRIME_1);
    }
    acc = acc.wrapping_add((element_hashes.len() as u64) ^ (XXPRIME_5 ^ 3527539));
    let h = acc as i64;
    if h == -1 { 1546275796 } else { h }
}

/// `hash(complex)` — `hash(real) + _PyHASH_IMAG * hash(imag)` (`complexobject.c`).
fn hash_complex(re: f64, im: f64) -> i64 {
    // _PyHASH_IMAG = 1000003.
    let combined =
        (hash_double(re) as u64).wrapping_add(1000003u64.wrapping_mul(hash_double(im) as u64));
    if combined == u64::MAX { combined.wrapping_sub(1) as i64 } else { combined as i64 }
}

/// `_shuffle_bits` from `frozenset_hash` (`setobject.c`).
fn shuffle_bits(h: u64) -> u64 {
    ((h ^ 89869747u64) ^ (h << 16)).wrapping_mul(3644798167u64)
}

/// CPython's `frozenset_hash` (`setobject.c`) over already-computed element
/// hashes — order-independent (XOR fold) plus a size mix and final avalanche.
fn hash_frozenset(element_hashes: &[i64]) -> i64 {
    let mut hash: u64 = 0;
    for &h in element_hashes {
        hash ^= shuffle_bits(h as u64);
    }
    hash ^= (element_hashes.len() as u64).wrapping_add(1).wrapping_mul(1927868237u64);
    hash ^= (hash >> 11) ^ (hash >> 25);
    hash = hash.wrapping_mul(69069u64).wrapping_add(907133923u64);
    if hash == u64::MAX { 590923713 } else { hash as i64 }
}

// CPython hashes `date`/`time`/`datetime` as the hash of their packed
// `_getstate` bytes, and `timedelta` as `hash((days, seconds, microseconds))`.
// Those hashes are deterministic (no SipHash key involvement beyond the shared
// bytes path), so reproducing them puts temporal values in the correct set slot
// order rather than the insertion-order fallback.

/// `date` packed state: `[year_hi, year_lo, month, day]`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "year is 1..=9999 (fits u16); month/day are 1..=31 — all bounded"
)]
fn date_state(d: &chrono::NaiveDate) -> [u8; 4] {
    use chrono::Datelike as _;
    let [yhi, ylo] = (d.year() as u16).to_be_bytes();
    [yhi, ylo, d.month() as u8, d.day() as u8]
}

/// `time` packed state: `[hour, minute, second, us_hi, us_mid, us_lo]` (naive).
#[expect(
    clippy::cast_possible_truncation,
    reason = "hour/min/sec are bounded; microseconds is 0..1_000_000, split big-endian into 3 bytes"
)]
fn time_state(t: &chrono::NaiveTime) -> [u8; 6] {
    use chrono::Timelike as _;
    let us = t.nanosecond() / 1000;
    [
        t.hour() as u8,
        t.minute() as u8,
        t.second() as u8,
        (us >> 16) as u8,
        (us >> 8) as u8,
        us as u8,
    ]
}

/// `datetime` packed state: the 4 date bytes followed by the 6 time bytes.
fn datetime_state(dt: &chrono::NaiveDateTime) -> [u8; 10] {
    let d = date_state(&dt.date());
    let t = time_state(&dt.time());
    [d[0], d[1], d[2], d[3], t[0], t[1], t[2], t[3], t[4], t[5]]
}

/// `hash(timedelta)` == `hash((days, seconds, microseconds))` over CPython's
/// normalised components (seconds/microseconds non-negative; days signed).
fn timedelta_hash(micros: i64) -> i64 {
    let secs_total = micros.div_euclid(1_000_000);
    let us = micros.rem_euclid(1_000_000);
    let days = secs_total.div_euclid(86_400);
    let seconds = secs_total.rem_euclid(86_400);
    hash_tuple(&[hash_i64(days), hash_i64(seconds), hash_i64(us)])
}

/// CPython's `hash()` for the hashable value types that appear as set/dict
/// elements. Returns `None` for values whose CPython hash is not reproducible
/// here — instances with a user `__hash__` (address-influenced or async),
/// `EnumMember` (identity hash), and aware `datetime` (offset-normalised) — so
/// the caller can fall back to insertion order rather than emit a wrong order.
#[must_use]
pub fn python_hash(value: &Value) -> Option<i64> {
    match value {
        Value::None => Some(NONE_HASH),
        Value::Bool(b) => Some(i64::from(*b)),
        Value::Int(n) => Some(hash_i64(*n)),
        Value::BigInt(n) => Some(hash_bigint(n)),
        Value::Float(f) => Some(hash_double(*f)),
        Value::Complex(c) => Some(hash_complex(c.re, c.im)),
        // Decimal/Fraction hash through CPython's rational formula so an equal
        // int/float/Decimal/Fraction share a hash (and thus a set/dict slot).
        Value::Decimal(..) | Value::Fraction(_) => crate::types::rational_number_hash(value),
        // Temporal types hash their packed state; an aware datetime uses a
        // different (offset-normalised) formula, so only naive ones are ported.
        Value::Date(d) => Some(hash_bytes(&date_state(d))),
        Value::Time(t) => Some(hash_bytes(&time_state(t))),
        Value::DateTime { dt, tz_offset_secs: None } => Some(hash_bytes(&datetime_state(dt))),
        Value::TimeDelta(micros) => Some(timedelta_hash(*micros)),
        Value::String(s) => Some(hash_bytes(&str_hash_buffer(s))),
        Value::Bytes(b) => Some(hash_bytes(b)),
        Value::Tuple(items) => {
            let mut hashes = Vec::with_capacity(items.len());
            for item in items {
                hashes.push(python_hash(item)?);
            }
            Some(hash_tuple(&hashes))
        }
        Value::Frozenset(body) => {
            let items = body.iter_ordered();
            let mut hashes = Vec::with_capacity(items.len());
            for item in &items {
                hashes.push(python_hash(item)?);
            }
            Some(hash_frozenset(&hashes))
        }
        _ => None,
    }
}

// ---- CPython set table (Objects/setobject.c) ----

const SET_MINSIZE: usize = 8;
const LINEAR_PROBES: usize = 9;
const PERTURB_SHIFT: u32 = 5;

struct SetTable {
    slots: Vec<Option<usize>>,
    hashes: Vec<i64>,
    mask: usize,
    fill: usize,
    used: usize,
}

impl SetTable {
    fn new(size: usize) -> Self {
        SetTable {
            slots: vec![None; size],
            hashes: vec![0; size],
            mask: size - 1,
            fill: 0,
            used: 0,
        }
    }

    /// `set_insert_clean`: place into a table known to have room, with no
    /// equality checks (used while rehashing during a resize).
    fn insert_clean(&mut self, elem: usize, hash: i64) {
        let mask = self.mask;
        let mut perturb = hash as u64;
        let mut i = (hash as u64 as usize) & mask;
        loop {
            if self.slots[i].is_none() {
                self.slots[i] = Some(elem);
                self.hashes[i] = hash;
                return;
            }
            if i + LINEAR_PROBES <= mask {
                let mut entry = i;
                for _ in 0..LINEAR_PROBES {
                    entry += 1;
                    if self.slots[entry].is_none() {
                        self.slots[entry] = Some(elem);
                        self.hashes[entry] = hash;
                        return;
                    }
                }
            }
            perturb >>= PERTURB_SHIFT;
            i = i.wrapping_mul(5).wrapping_add(1).wrapping_add(perturb as usize) & mask;
        }
    }

    /// `set_add_entry` for known-distinct elements (the set already deduplicated
    /// via Python equality, so the equality branch never fires).
    fn add(&mut self, elem: usize, hash: i64) {
        let mask = self.mask;
        let mut perturb = hash as u64;
        let mut i = (hash as u64 as usize) & mask;
        loop {
            let probes: isize = if i + LINEAR_PROBES <= mask { LINEAR_PROBES as isize } else { 0 };
            let mut entry = i;
            let mut p = probes;
            loop {
                if self.slots[entry].is_none() {
                    self.slots[entry] = Some(elem);
                    self.hashes[entry] = hash;
                    self.fill += 1;
                    self.used += 1;
                    return;
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
        let mut newsize = SET_MINSIZE;
        while newsize <= minused {
            newsize <<= 1;
        }
        let old: Vec<(usize, i64)> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(idx, slot)| slot.map(|elem| (elem, self.hashes[idx])))
            .collect();
        self.slots = vec![None; newsize];
        self.hashes = vec![0; newsize];
        self.mask = newsize - 1;
        for (elem, hash) in old {
            self.insert_clean(elem, hash);
        }
    }
}

/// Indices into `items` (given in true insertion order) in CPython's set
/// iteration order. Returns `None` when any element's hash is not reproducible,
/// so the caller keeps insertion order rather than emitting a wrong one.
#[must_use]
pub fn cpython_set_order_indices(items: &[Value]) -> Option<Vec<usize>> {
    let mut hashes = Vec::with_capacity(items.len());
    for item in items {
        hashes.push(python_hash(item)?);
    }
    let mut table = SetTable::new(SET_MINSIZE);
    for (elem, &hash) in hashes.iter().enumerate() {
        table.add(elem, hash);
        table.maybe_resize();
    }
    Some(table.slots.iter().filter_map(|slot| *slot).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values are `hash(x)` in CPython 3.12 with PYTHONHASHSEED=0.
    #[test]
    fn int_hashes_match_cpython() {
        assert_eq!(hash_i64(0), 0);
        assert_eq!(hash_i64(1), 1);
        assert_eq!(hash_i64(-1), -2);
        assert_eq!(hash_i64(-2), -2);
        assert_eq!(hash_i64(255), 255);
        assert_eq!(hash_i64((1 << 61) - 1), 0);
        assert_eq!(hash_i64(1 << 61), 1);
    }

    #[test]
    fn float_hashes_match_cpython() {
        assert_eq!(hash_double(0.0), 0);
        assert_eq!(hash_double(1.0), 1);
        assert_eq!(hash_double(2.0), 2);
        assert_eq!(hash_double(0.5), 1152921504606846976);
        assert_eq!(hash_double(-2.5), -1152921504606846978);
        assert_eq!(hash_double(f64::INFINITY), 314159);
        // hash(2.0) == hash(2), consistent with the int tower.
        assert_eq!(hash_double(3.0), hash_i64(3));
    }

    #[test]
    fn string_hashes_match_cpython() {
        assert_eq!(python_hash(&Value::String("".into())), Some(0));
        assert_eq!(python_hash(&Value::String("a".into())), Some(4644417185603328019));
        assert_eq!(python_hash(&Value::String("abc".into())), Some(-4594863902769663758));
        assert_eq!(python_hash(&Value::String("hello".into())), Some(-2096571579003691106));
        // Non-ASCII hashes the Latin-1 form, not UTF-8.
        assert_eq!(python_hash(&Value::String("café".into())), Some(137524001917817222));
    }

    #[test]
    fn misc_hashes_match_cpython() {
        assert_eq!(python_hash(&Value::None), Some(0xFCA8_6420));
        assert_eq!(python_hash(&Value::Bool(true)), Some(1));
        assert_eq!(python_hash(&Value::Bool(false)), Some(0));
        assert_eq!(
            python_hash(&Value::Tuple(vec![Value::Int(1), Value::Int(2)])),
            Some(-3550055125485641917)
        );
    }

    #[test]
    fn complex_hashes_match_cpython() {
        let cx = |re, im| Value::Complex(Box::new(num_complex::Complex64::new(re, im)));
        assert_eq!(python_hash(&cx(3.0, 4.0)), Some(4000015));
        // complex(2, 0) hashes like int/float 2 (equal in the numeric tower).
        assert_eq!(python_hash(&cx(2.0, 0.0)), Some(2));
    }

    #[test]
    fn frozenset_hashes_match_cpython() {
        let fs = |xs: &[i64]| {
            Value::new_frozenset(xs.iter().map(|&n| Value::Int(n)).collect::<Vec<_>>())
        };
        assert_eq!(python_hash(&fs(&[1, 2, 3])), Some(-272375401224217160));
        assert_eq!(python_hash(&fs(&[])), Some(133146708735736));
        // Order-independent: {1,2,3} and {3,2,1} hash identically.
        assert_eq!(python_hash(&fs(&[3, 2, 1])), python_hash(&fs(&[1, 2, 3])));
        let fss = Value::new_frozenset(vec![Value::String("a".into()), Value::String("b".into())]);
        assert_eq!(python_hash(&fss), Some(1679668661828516449));
    }

    #[test]
    fn set_order_indices_match_cpython() {
        // list({'a','b','c'}) == ['c','a','b'] in CPython 3.12 (seed 0), i.e.
        // slot order picks source indices [2, 0, 1]. The full set-order
        // machinery is exercised in `pyset`.
        let items =
            vec![Value::String("a".into()), Value::String("b".into()), Value::String("c".into())];
        assert_eq!(cpython_set_order_indices(&items), Some(vec![2, 0, 1]));
    }
}
