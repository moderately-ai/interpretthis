// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `random` module.
//!
//! Backed by a faithful port of CPython's Mersenne Twister (MT19937,
//! `_randommodule.c`) so an int-seeded sequence reproduces CPython
//! bit-for-bit: `random.seed(n)` uses the same `init_by_array` key
//! expansion, `random()` uses the same 53-bit `genrand_res53`, and the
//! integer methods (`randrange`/`randint`/`choice`/`shuffle`/`sample`)
//! build on the same `getrandbits`-with-rejection `_randbelow`. Unseeded
//! use is deterministic from a fixed default seed (CPython's unseeded RNG
//! draws from the OS and is non-reproducible either way).

use num_bigint::BigInt;
use num_traits::Signed as _;

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    value::{Value, shared_list},
};

const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

/// CPython Mersenne Twister state.
#[derive(Debug, Clone)]
pub struct MtState {
    mt: [u32; N],
    mti: usize,
}

impl Default for MtState {
    fn default() -> Self {
        Self::new()
    }
}

impl MtState {
    #[must_use]
    pub fn new() -> Self {
        let mut s = Self { mt: [0u32; N], mti: N + 1 };
        // Deterministic default so unseeded use is at least reproducible
        // within a run (CPython seeds from the OS; unseeded values are never
        // compared for parity).
        s.seed_from_u64(5489);
        s
    }

    fn init_genrand(&mut self, seed: u32) {
        self.mt[0] = seed;
        for i in 1..N {
            let prev = self.mt[i - 1];
            self.mt[i] = 1_812_433_253u32
                .wrapping_mul(prev ^ (prev >> 30))
                .wrapping_add(u32::try_from(i).unwrap_or(0));
        }
        self.mti = N;
    }

    fn init_by_array(&mut self, init_key: &[u32]) {
        self.init_genrand(19_650_218);
        let key_len = init_key.len().max(1);
        let mut i = 1usize;
        let mut j = 0usize;
        let mut k = N.max(key_len);
        while k > 0 {
            let prev = self.mt[i - 1];
            self.mt[i] = (self.mt[i] ^ ((prev ^ (prev >> 30)).wrapping_mul(1_664_525)))
                .wrapping_add(*init_key.get(j).unwrap_or(&0))
                .wrapping_add(u32::try_from(j).unwrap_or(0));
            i += 1;
            j += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
            if j >= init_key.len() {
                j = 0;
            }
            k -= 1;
        }
        k = N - 1;
        while k > 0 {
            let prev = self.mt[i - 1];
            self.mt[i] = (self.mt[i] ^ ((prev ^ (prev >> 30)).wrapping_mul(1_566_083_941)))
                .wrapping_sub(u32::try_from(i).unwrap_or(0));
            i += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
            k -= 1;
        }
        self.mt[0] = 0x8000_0000;
    }

    /// Seed from a non-negative integer's little-endian 32-bit words, exactly
    /// as CPython's `random_seed` does for an int argument (using `abs`).
    fn seed_from_bigint(&mut self, n: &BigInt) {
        let magnitude = n.abs();
        let (_, bytes_le) = magnitude.to_bytes_le();
        // Pack little-endian bytes into little-endian u32 words.
        let mut key: Vec<u32> = Vec::with_capacity(bytes_le.len() / 4 + 1);
        for chunk in bytes_le.chunks(4) {
            let mut w = 0u32;
            for (i, &b) in chunk.iter().enumerate() {
                w |= u32::from(b) << (8 * i);
            }
            key.push(w);
        }
        if key.is_empty() {
            key.push(0);
        }
        self.init_by_array(&key);
    }

    fn seed_from_u64(&mut self, n: u64) {
        self.seed_from_bigint(&BigInt::from(n));
    }

    fn genrand_uint32(&mut self) -> u32 {
        if self.mti >= N {
            let mag01 = [0u32, MATRIX_A];
            for kk in 0..N - M {
                let y = (self.mt[kk] & UPPER_MASK) | (self.mt[kk + 1] & LOWER_MASK);
                self.mt[kk] = self.mt[kk + M] ^ (y >> 1) ^ mag01[(y & 1) as usize];
            }
            for kk in N - M..N - 1 {
                let y = (self.mt[kk] & UPPER_MASK) | (self.mt[kk + 1] & LOWER_MASK);
                self.mt[kk] = self.mt[kk + M - N] ^ (y >> 1) ^ mag01[(y & 1) as usize];
            }
            let y = (self.mt[N - 1] & UPPER_MASK) | (self.mt[0] & LOWER_MASK);
            self.mt[N - 1] = self.mt[M - 1] ^ (y >> 1) ^ mag01[(y & 1) as usize];
            self.mti = 0;
        }
        let mut y = self.mt[self.mti];
        self.mti += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// `random.random()` — a float in [0, 1) with 53 bits of precision
    /// (`genrand_res53`).
    fn random(&mut self) -> f64 {
        let a = self.genrand_uint32() >> 5;
        let b = self.genrand_uint32() >> 6;
        (f64::from(a) * 67_108_864.0 + f64::from(b)) / 9_007_199_254_740_992.0
    }

    /// `random.getrandbits(k)` — a non-negative integer with `k` random bits.
    fn getrandbits(&mut self, k: u32) -> BigInt {
        if k == 0 {
            return BigInt::from(0);
        }
        if k <= 32 {
            return BigInt::from(self.genrand_uint32() >> (32 - k));
        }
        let words = (k - 1) / 32 + 1;
        let mut result = BigInt::from(0);
        let mut remaining = k;
        for i in 0..words {
            let mut r = self.genrand_uint32();
            if remaining < 32 {
                r >>= 32 - remaining;
            }
            result |= BigInt::from(r) << (32 * i);
            remaining = remaining.saturating_sub(32);
        }
        result
    }

    /// `_randbelow(n)` — a uniform integer in [0, n) via `getrandbits`
    /// rejection sampling, matching CPython. `n` must be positive.
    fn randbelow(&mut self, n: &BigInt) -> BigInt {
        if !n.is_positive() {
            return BigInt::from(0);
        }
        let k = n.bits(); // bit_length
        #[allow(clippy::cast_possible_truncation)]
        let k = k as u32;
        loop {
            let r = self.getrandbits(k);
            if &r < n {
                return r;
            }
        }
    }
}

fn as_bigint(v: &Value) -> Option<BigInt> {
    match v {
        Value::Int(i) => Some(BigInt::from(*i)),
        Value::BigInt(b) => Some((**b).clone()),
        Value::Bool(b) => Some(BigInt::from(i64::from(*b))),
        _ => None,
    }
}

fn bigint_to_value(b: BigInt) -> Value {
    i64::try_from(&b).map_or_else(|_| Value::BigInt(Box::new(b)), Value::Int)
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        Value::Bool(b) => Some(f64::from(*b)),
        Value::BigInt(b) => Some(bigint_to_f64(b)),
        _ => None,
    }
}

fn bigint_to_f64(b: &BigInt) -> f64 {
    b.to_string().parse().unwrap_or(0.0)
}

/// Materialise a `population` argument (list/tuple/range/str) into an indexable
/// vector, matching how CPython's `choice`/`sample` consume a sequence.
fn population_items(v: &Value) -> Result<Vec<Value>, EvalError> {
    match v {
        Value::List(items) => Ok(items.lock().clone()),
        Value::Tuple(items) => Ok(items.clone()),
        Value::String(s) => Ok(s.chars().map(|c| Value::String(c.to_string().into())).collect()),
        Value::Range { start, stop, step } => {
            Ok(crate::eval::control_flow::iterate_value(&Value::Range {
                start: *start,
                stop: *stop,
                step: *step,
            })?)
        }
        other => crate::eval::control_flow::iterate_value(other),
    }
}

pub fn has_function(name: &str) -> bool {
    matches!(
        name,
        "seed"
            | "random"
            | "getrandbits"
            | "randint"
            | "randrange"
            | "choice"
            | "shuffle"
            | "sample"
            | "uniform"
    )
}

#[allow(clippy::too_many_lines)]
pub fn call(state: &mut crate::state::InterpreterState, func: &str, args: &[Value]) -> EvalResult {
    let rng = &mut state.random_state;
    match func {
        "seed" => {
            match args.first() {
                None | Some(Value::None) => {
                    // Non-reproducible in CPython; use a fixed default so our
                    // behaviour is at least deterministic within the run.
                    rng.seed_from_u64(5489);
                }
                Some(v) => {
                    let n = as_bigint(v).ok_or_else(|| {
                        EvalError::from(InterpreterError::TypeError(
                            "random.seed() currently supports integer seeds".into(),
                        ))
                    })?;
                    rng.seed_from_bigint(&n);
                }
            }
            Ok(Value::None)
        }
        "random" => Ok(Value::Float(rng.random())),
        "getrandbits" => {
            let k = args.first().and_then(as_bigint).ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "getrandbits() requires an integer".into(),
                ))
            })?;
            if !k.is_positive() && k != BigInt::from(0) {
                return Err(InterpreterError::ValueError(
                    "number of bits must be non-negative".into(),
                )
                .into());
            }
            let k = u32::try_from(&k).map_err(|_| {
                EvalError::from(InterpreterError::ValueError("number of bits too large".into()))
            })?;
            Ok(bigint_to_value(rng.getrandbits(k)))
        }
        "randint" => {
            // randint(a, b) == randrange(a, b+1): inclusive on both ends.
            let a = args.first().and_then(as_bigint);
            let b = args.get(1).and_then(as_bigint);
            let (Some(a), Some(b)) = (a, b) else {
                return Err(
                    InterpreterError::TypeError("randint() requires two integers".into()).into()
                );
            };
            if b < a {
                return Err(InterpreterError::ValueError("empty range for randint()".into()).into());
            }
            let width = &b - &a + BigInt::from(1);
            Ok(bigint_to_value(a + rng.randbelow(&width)))
        }
        "randrange" => {
            let start = args.first().and_then(as_bigint).ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "randrange() requires integer arguments".into(),
                ))
            })?;
            let (lo, hi, step) =
                match (args.get(1).and_then(as_bigint), args.get(2).and_then(as_bigint)) {
                    // randrange(stop)
                    (None, _) if args.len() == 1 => {
                        (BigInt::from(0), start.clone(), BigInt::from(1))
                    }
                    // randrange(start, stop[, step])
                    (Some(stop), maybe_step) => {
                        (start.clone(), stop, maybe_step.unwrap_or_else(|| BigInt::from(1)))
                    }
                    _ => (BigInt::from(0), start.clone(), BigInt::from(1)),
                };
            if step == BigInt::from(0) {
                return Err(InterpreterError::ValueError(
                    "randrange() arg 3 must not be zero".into(),
                )
                .into());
            }
            // width = number of valid values = ceil((hi - lo) / step) for the
            // step's sign; CPython computes n = (hi-lo+step-(step>0?1:-1))/step.
            let span = &hi - &lo;
            let n = if step.is_positive() {
                (&span + &step - BigInt::from(1)) / &step
            } else {
                (&span + &step + BigInt::from(1)) / &step
            };
            if !n.is_positive() {
                return Err(
                    InterpreterError::ValueError("empty range for randrange()".into()).into()
                );
            }
            Ok(bigint_to_value(lo + step * rng.randbelow(&n)))
        }
        "uniform" => {
            let a = args.first().and_then(as_f64);
            let b = args.get(1).and_then(as_f64);
            let (Some(a), Some(b)) = (a, b) else {
                return Err(
                    InterpreterError::TypeError("uniform() requires two numbers".into()).into()
                );
            };
            Ok(Value::Float(a + (b - a) * rng.random()))
        }
        "choice" => {
            let seq = args.first().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError("choice() requires a sequence".into()))
            })?;
            let items = population_items(seq)?;
            if items.is_empty() {
                return Err(EvalError::Exception(crate::value::ExceptionValue::new(
                    "IndexError",
                    "Cannot choose from an empty sequence",
                )));
            }
            let idx = rng.randbelow(&BigInt::from(items.len()));
            let idx = usize::try_from(&idx).unwrap_or(0);
            Ok(items[idx].clone())
        }
        "shuffle" => {
            // In-place Fisher-Yates over the shared list (reference-semantic).
            let Some(Value::List(shared)) = args.first() else {
                return Err(InterpreterError::TypeError("shuffle() requires a list".into()).into());
            };
            let len = shared.lock().len();
            for i in (1..len).rev() {
                let j = rng.randbelow(&BigInt::from(i + 1));
                let j = usize::try_from(&j).unwrap_or(0);
                shared.lock().swap(i, j);
            }
            Ok(Value::None)
        }
        "sample" => {
            let seq = args.first().ok_or_else(|| {
                EvalError::from(InterpreterError::TypeError(
                    "sample() requires a population".into(),
                ))
            })?;
            let k = args.get(1).and_then(as_bigint).and_then(|b| usize::try_from(&b).ok());
            let Some(k) = k else {
                return Err(
                    InterpreterError::TypeError("sample() requires an integer k".into()).into()
                );
            };
            let population = population_items(seq)?;
            let n = population.len();
            if k > n {
                return Err(InterpreterError::ValueError(
                    "Sample larger than population or is negative".into(),
                )
                .into());
            }
            let mut result: Vec<Value> = Vec::with_capacity(k);
            // CPython's set-size heuristic selects a pool-based or a
            // selected-set algorithm; both consume `_randbelow` identically.
            let mut setsize = 21usize;
            if k > 5 {
                let extra = 4f64.powf((k as f64 * 3.0).log(4.0).ceil());
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    setsize += extra as usize;
                }
            }
            if n <= setsize {
                let mut pool: Vec<Value> = population;
                for i in 0..k {
                    let j = rng.randbelow(&BigInt::from(n - i));
                    let j = usize::try_from(&j).unwrap_or(0);
                    result.push(pool[j].clone());
                    pool[j] = pool[n - i - 1].clone();
                }
            } else {
                let mut selected: std::collections::HashSet<usize> =
                    std::collections::HashSet::new();
                for _ in 0..k {
                    let mut j = usize::try_from(&rng.randbelow(&BigInt::from(n))).unwrap_or(0);
                    while selected.contains(&j) {
                        j = usize::try_from(&rng.randbelow(&BigInt::from(n))).unwrap_or(0);
                    }
                    selected.insert(j);
                    result.push(population[j].clone());
                }
            }
            Ok(Value::List(shared_list(result)))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'random' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// `random` module registration.
pub struct RandomModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for RandomModule {
    fn name(&self) -> &'static str {
        "random"
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &indexmap::IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        call(state, func, args)
    }
}
