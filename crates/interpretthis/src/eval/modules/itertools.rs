// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `itertools` module.
//!
//! Over a *lazy* input (a generator / lazy iterator / `count`-`cycle`-`repeat`)
//! the consumers `islice`/`takewhile`/`dropwhile`/`filterfalse`/`starmap`/
//! `accumulate`/`pairwise`/`compress` return a synthesized generator that steps
//! the source one item at a time — so an early stop does not over-run (or hang
//! on) an infinite source, and loop-var closures keep their interleaved-capture
//! timing. Over a finite / non-lazy list input they materialise eagerly (same
//! eager shape as the rest of the module). `count`/`cycle`/`repeat` are the lazy
//! producers ([`Value::BuiltinIter`]); the combinatorics / `chain` / `tee` /
//! `batched` / `zip_longest` families stay eager (they consume the whole input
//! anyway). Infinite iterators bound themselves against the interpreter's
//! operation counter rather than running forever.

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{
        control_flow::iterate_value,
        modules::{need_arg, value_error},
    },
    value::{Value, shared_list},
};

pub fn has_function(name: &str) -> bool {
    matches!(
        name,
        "chain"
            | "combinations"
            | "combinations_with_replacement"
            | "permutations"
            | "product"
            | "repeat"
            | "count"
            | "cycle"
            | "islice"
            | "takewhile"
            | "dropwhile"
            | "compress"
            | "accumulate"
            | "groupby"
            | "zip_longest"
            | "starmap"
            | "pairwise"
            | "filterfalse"
            | "tee"
            | "batched"
            | "chain.from_iterable"
    )
}

pub fn call(func: &str, args: &[Value], kwargs: &indexmap::IndexMap<String, Value>) -> EvalResult {
    match func {
        "chain" => {
            // chain(*iterables) — concatenate every iterable arg.
            let mut out: Vec<Value> = Vec::new();
            for arg in args {
                out.extend(iterate_value(arg)?);
            }
            Ok(Value::List(shared_list(out)))
        }
        "chain.from_iterable" => {
            // chain.from_iterable(iterable) — flatten one level: iterate the
            // single argument and concatenate each sub-iterable in order.
            let mut out: Vec<Value> = Vec::new();
            for sub in iterate_value(need_arg("chain.from_iterable", args, 0)?)? {
                out.extend(iterate_value(&sub)?);
            }
            Ok(Value::List(shared_list(out)))
        }
        "batched" => {
            // batched(iterable, n) — consecutive n-length tuples; the final
            // batch is short. n must be at least 1 (CPython 3.12+).
            let items = iterate_value(need_arg("batched", args, 0)?)?;
            let n = arg_usize("batched", args, 1)?;
            if n == 0 {
                return Err(InterpreterError::ValueError("n must be at least one".into()).into());
            }
            let batches = items.chunks(n).map(|c| Value::Tuple(c.to_vec())).collect::<Vec<_>>();
            Ok(Value::List(shared_list(batches)))
        }
        "tee" => {
            // tee(iterable, n=2) — n independent iterators over the same items.
            // The eager model returns n separate list copies (matching how
            // `chain` etc. materialise), so each iterates independently.
            let items = iterate_value(need_arg(func, args, 0)?)?;
            let n = match args.get(1) {
                None => 2,
                Some(Value::Bool(b)) => usize::from(*b),
                Some(Value::Int(k)) if *k >= 0 => usize::try_from(*k).unwrap_or(0),
                Some(Value::Int(_)) => {
                    return Err(InterpreterError::ValueError("n must be >= 0".into()).into());
                }
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "tee() n must be an integer (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            let copies = (0..n).map(|_| Value::List(shared_list(items.clone()))).collect();
            Ok(Value::Tuple(copies))
        }
        "repeat" => {
            // repeat(obj, [times]) — bounded by `times`; unbounded
            // form rejected to keep the eager shape safe.
            let obj = need_arg(func, args, 0)?.clone();
            let times = match args.get(1) {
                Some(Value::Int(n)) => usize::try_from(*n).unwrap_or(0),
                Some(Value::Bool(b)) => usize::from(*b),
                None => {
                    return Err(InterpreterError::Runtime(
                        "itertools.repeat without a count is not supported (would not terminate); pass a `times` argument (see CONFORMANCE.md#unsupported-language-features)".into(),
                    )
                    .into());
                }
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "repeat() times must be an integer (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            Ok(Value::List(shared_list(std::iter::repeat_n(obj, times).collect())))
        }
        // `count` / `cycle` are the unbounded lazy producers; they need
        // interpreter state to register their cursor, so they are
        // handled in the async `Module::call` and never reach here.
        "count" | "cycle" => Err(InterpreterError::Runtime(format!(
            "itertools.{func} must be evaluated through the module dispatch"
        ))
        .into()),
        "islice" => {
            // islice(iterable, stop) or islice(iterable, start, stop, [step]).
            // Bounds are validated: start/stop must be a non-negative int or
            // None, step a positive int or None (CPython raises ValueError).
            let items = iterate_value(need_arg(func, args, 0)?)?;
            let (start, stop, step) = match args.len() {
                2 => (0usize, islice_bound(args.get(1), items.len(), "Stop")?, 1usize),
                3 => (
                    islice_bound(args.get(1), 0, "Start")?,
                    islice_bound(args.get(2), items.len(), "Stop")?,
                    1usize,
                ),
                4 => (
                    islice_bound(args.get(1), 0, "Start")?,
                    islice_bound(args.get(2), items.len(), "Stop")?,
                    islice_step(args.get(3))?,
                ),
                _ => {
                    return Err(InterpreterError::TypeError(
                        "islice() requires 2-4 arguments".into(),
                    )
                    .into());
                }
            };
            let stop = stop.min(items.len());
            let mut out = Vec::new();
            let mut idx = start;
            while idx < stop {
                out.push(items[idx].clone());
                idx += step;
            }
            Ok(Value::List(shared_list(out)))
        }
        "combinations" => {
            // combinations(iterable, r) — all r-length tuples in
            // lexicographic order, no repeats.
            let items = iterate_value(need_arg(func, args, 0)?)?;
            let r = arg_usize("combinations", args, 1)?;
            Ok(Value::List(shared_list(combinations(&items, r))))
        }
        "combinations_with_replacement" => {
            // Like combinations, but an element may be chosen more than once.
            let items = iterate_value(need_arg(func, args, 0)?)?;
            let r = arg_usize("combinations_with_replacement", args, 1)?;
            Ok(Value::List(shared_list(combinations_with_replacement(&items, r))))
        }
        "permutations" => {
            // permutations(iterable, [r]) — all r-length permutations.
            let items = iterate_value(need_arg(func, args, 0)?)?;
            let r = match args.get(1) {
                None | Some(Value::None) => items.len(),
                Some(Value::Bool(b)) => usize::from(*b),
                // A negative r is a ValueError (not "treat as len"); a huge r
                // that overflows usize is likewise out of range.
                Some(Value::Int(n)) => usize::try_from(*n)
                    .map_err(|_| value_error("permutations() r must be non-negative"))?,
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "permutations() r must be an integer (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            Ok(Value::List(shared_list(permutations(&items, r))))
        }
        "product" => {
            // product(*iterables, repeat=1) — Cartesian product of the pools
            // repeated `repeat` times (`product(a, b, repeat=2)` pools are
            // [a, b, a, b]).
            let repeat = match kwargs.get("repeat") {
                None => 1,
                Some(Value::Int(n)) => usize::try_from(*n)
                    .map_err(|_| value_error("product() repeat must be non-negative"))?,
                Some(Value::Bool(b)) => usize::from(*b),
                Some(other) => {
                    return Err(InterpreterError::TypeError(format!(
                        "product() repeat must be an integer (got '{}')",
                        other.type_name()
                    ))
                    .into());
                }
            };
            if let Some(bad) = kwargs.keys().find(|k| k.as_str() != "repeat") {
                return Err(InterpreterError::TypeError(format!(
                    "product() got an unexpected keyword argument '{bad}'"
                ))
                .into());
            }
            let base: Vec<Vec<Value>> =
                args.iter().map(iterate_value).collect::<Result<Vec<_>, _>>()?;
            let pools: Vec<Vec<Value>> = std::iter::repeat_n(base, repeat).flatten().collect();
            Ok(Value::List(shared_list(cartesian_product(&pools))))
        }
        "zip_longest" => {
            // zip_longest(*iterables, fillvalue=None) — zip to the
            // longest input, padding short ones with `fillvalue`.
            if let Some(bad) = kwargs.keys().find(|k| k.as_str() != "fillvalue") {
                return Err(InterpreterError::TypeError(format!(
                    "zip_longest() got an unexpected keyword argument '{bad}'"
                ))
                .into());
            }
            let fill = kwargs.get("fillvalue").cloned().unwrap_or(Value::None);
            let pools: Vec<Vec<Value>> =
                args.iter().map(iterate_value).collect::<Result<Vec<_>, _>>()?;
            let max_len = pools.iter().map(Vec::len).max().unwrap_or(0);
            let mut out = Vec::with_capacity(max_len);
            for i in 0..max_len {
                let row: Vec<Value> = pools
                    .iter()
                    .map(|p| p.get(i).cloned().unwrap_or_else(|| fill.clone()))
                    .collect();
                out.push(Value::Tuple(row));
            }
            Ok(Value::List(shared_list(out)))
        }
        "pairwise" => {
            // pairwise(iterable) — successive overlapping pairs.
            let items = iterate_value(need_arg(func, args, 0)?)?;
            let out: Vec<Value> =
                items.windows(2).map(|w| Value::Tuple(vec![w[0].clone(), w[1].clone()])).collect();
            Ok(Value::List(shared_list(out)))
        }
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'itertools' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// `combinations(items, r)`: all r-length sub-tuples in
/// lexicographic order.
/// `combinations_with_replacement(iterable, r)` — sorted r-length tuples where
/// each element may repeat (indices are non-decreasing, all 0..n each usable).
fn combinations_with_replacement(items: &[Value], r: usize) -> Vec<Value> {
    let n = items.len();
    if n == 0 {
        return if r == 0 { vec![Value::Tuple(Vec::new())] } else { Vec::new() };
    }
    let mut result = Vec::new();
    let mut indices = vec![0usize; r];
    loop {
        result.push(Value::Tuple(indices.iter().map(|&i| items[i].clone()).collect()));
        // Advance: rightmost index not yet at n-1 increments; the rest reset to it.
        let mut i = r;
        loop {
            if i == 0 {
                return result;
            }
            i -= 1;
            if indices[i] != n - 1 {
                let v = indices[i] + 1;
                for slot in &mut indices[i..] {
                    *slot = v;
                }
                break;
            }
        }
    }
}

fn combinations(items: &[Value], r: usize) -> Vec<Value> {
    if r > items.len() {
        return Vec::new();
    }
    if r == 0 {
        return vec![Value::Tuple(Vec::new())];
    }
    let mut result = Vec::new();
    let n = items.len();
    let mut indices: Vec<usize> = (0..r).collect();
    loop {
        let combo: Vec<Value> = indices.iter().map(|&i| items[i].clone()).collect();
        result.push(Value::Tuple(combo));
        // Advance: find rightmost index that can be incremented.
        let mut i = r;
        while i > 0 {
            i -= 1;
            if indices[i] != i + n - r {
                indices[i] += 1;
                for j in (i + 1)..r {
                    indices[j] = indices[j - 1] + 1;
                }
                break;
            }
            if i == 0 {
                return result;
            }
        }
    }
}

/// `permutations(items, r)`: all r-length orderings.
fn permutations(items: &[Value], r: usize) -> Vec<Value> {
    if r > items.len() {
        return Vec::new();
    }
    if r == 0 {
        return vec![Value::Tuple(Vec::new())];
    }
    let mut result = Vec::new();
    let n = items.len();
    let mut indices: Vec<usize> = (0..n).collect();
    let mut cycles: Vec<usize> = (n - r + 1..=n).rev().collect();
    result.push(Value::Tuple(indices.iter().take(r).map(|&i| items[i].clone()).collect()));
    loop {
        let mut done = true;
        for i in (0..r).rev() {
            cycles[i] -= 1;
            if cycles[i] == 0 {
                let removed = indices.remove(i);
                indices.push(removed);
                cycles[i] = n - i;
            } else {
                let j = indices.len() - cycles[i];
                indices.swap(i, j);
                result.push(Value::Tuple(
                    indices.iter().take(r).map(|&k| items[k].clone()).collect(),
                ));
                done = false;
                break;
            }
        }
        if done {
            return result;
        }
    }
}

/// `product(*pools)`: Cartesian product.
fn cartesian_product(pools: &[Vec<Value>]) -> Vec<Value> {
    if pools.is_empty() {
        return vec![Value::Tuple(Vec::new())];
    }
    if pools.iter().any(Vec::is_empty) {
        return Vec::new();
    }
    let mut result = vec![Vec::new()];
    for pool in pools {
        let mut next = Vec::new();
        for combo in &result {
            for item in pool {
                let mut extended = combo.clone();
                extended.push(item.clone());
                next.push(extended);
            }
        }
        result = next;
    }
    result.into_iter().map(Value::Tuple).collect()
}

/// An `islice` start/stop bound: absent or `None` uses `default`; a
/// non-negative integer is that value; anything else (negative, float, ...)
/// raises `ValueError`, matching CPython.
fn islice_bound(arg: Option<&Value>, default: usize, which: &str) -> Result<usize, EvalError> {
    match arg {
        None | Some(Value::None) => Ok(default),
        Some(Value::Int(n)) if *n >= 0 => Ok(usize::try_from(*n).unwrap_or(usize::MAX)),
        Some(Value::Bool(b)) => Ok(usize::from(*b)),
        _ => Err(value_error(format!(
            "{which} argument for islice() must be None or an integer: 0 <= x <= sys.maxsize."
        ))),
    }
}

/// The `islice` step: absent or `None` is 1; otherwise a positive integer.
/// Zero, negative, or non-integer raises `ValueError`.
fn islice_step(arg: Option<&Value>) -> Result<usize, EvalError> {
    match arg {
        None | Some(Value::None) => Ok(1),
        Some(Value::Int(n)) if *n >= 1 => Ok(usize::try_from(*n).unwrap_or(usize::MAX)),
        Some(Value::Bool(true)) => Ok(1),
        _ => Err(value_error("Step for islice() must be a positive integer or None.")),
    }
}

fn arg_usize(func: &str, args: &[Value], index: usize) -> Result<usize, EvalError> {
    match args.get(index) {
        Some(Value::Int(n)) => usize::try_from(*n).map_err(|_| {
            EvalError::from(InterpreterError::ValueError(format!("{func}() argument out of range")))
        }),
        Some(Value::Bool(b)) => Ok(usize::from(*b)),
        _ => Err(InterpreterError::TypeError(format!(
            "{func}() missing or non-integer argument at position {index}"
        ))
        .into()),
    }
}

/// `compress(data, selectors)`: yield `data[i]` for each `selectors[i]`
/// that is truthy. Eager: materialises the entire output list.
fn compress_impl(args: &[Value]) -> EvalResult {
    let data_val = need_arg("compress", args, 0)?;
    let selectors_val = need_arg("compress", args, 1)?;
    let data = iterate_value(data_val)?;
    let selectors = iterate_value(selectors_val)?;
    let mut out = Vec::new();
    for (item, sel) in data.into_iter().zip(selectors) {
        if sel.is_truthy() {
            out.push(item);
        }
    }
    Ok(Value::List(shared_list(out)))
}

/// `takewhile(predicate, iterable)`: yield items until predicate returns
/// falsy. Re-enters the evaluator for each predicate call.
/// `islice(iterable, stop)` / `islice(iterable, start, stop[, step])` with lazy
/// consumption for generator/lazy-iterator inputs — it pulls at most `stop`
/// items via the generator's single-step primitive rather than materialising
/// the whole (possibly infinite) input. Non-lazy inputs use the eager path.
async fn islice_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let input = need_arg("islice", args, 0)?;
    if !matches!(input, Value::Generator { .. } | Value::Lazy { .. } | Value::BuiltinIter { .. }) {
        return call("islice", args, &indexmap::IndexMap::new());
    }
    let bound = |v: Option<&Value>| -> Result<Option<usize>, EvalError> {
        match v {
            None | Some(Value::None) => Ok(None),
            Some(Value::Int(n)) if *n >= 0 => Ok(Some(usize::try_from(*n).unwrap_or(usize::MAX))),
            Some(Value::Bool(b)) => Ok(Some(usize::from(*b))),
            _ => Err(value_error(
                "Indices for islice() must be None or an integer: 0 <= x <= sys.maxsize.",
            )),
        }
    };
    let (start, stop, step) = match args.len() {
        2 => (0usize, bound(args.get(1))?, 1usize),
        3 => (bound(args.get(1))?.unwrap_or(0), bound(args.get(2))?, 1usize),
        4 => {
            let step = match bound(args.get(3))? {
                Some(0) => {
                    return Err(value_error(
                        "Step for islice() must be a positive integer or None.",
                    ));
                }
                Some(s) => s,
                None => 1,
            };
            (bound(args.get(1))?.unwrap_or(0), bound(args.get(2))?, step)
        }
        _ => {
            return Err(
                InterpreterError::TypeError("islice() requires 2-4 arguments".into()).into()
            );
        }
    };
    let input = input.clone();

    // Synthesize a lazy generator equivalent to
    //   .idx = 0
    //   for .item in .0:
    //       if .idx >= stop: break                # when stop is bounded
    //       if .idx >= start and (.idx-start) % step == 0: yield .item
    //       .idx = .idx + 1
    // so the slice is produced ONE item at a time, interleaved with the
    // consumer. This preserves CPython's laziness — an early stop doesn't over-
    // run the source, and loop-variable closures captured in the source see
    // their interleaved value rather than the final one.
    if let Some(generator) = synthesize_islice(state, &input, start, stop, step) {
        return Ok(generator);
    }

    // Fallback (body not suspendable — should not happen): eager pull.
    let empty = indexmap::IndexMap::new();
    let mut out = Vec::new();
    let mut idx = 0usize;
    loop {
        if stop.is_some_and(|s| idx >= s) {
            break;
        }
        let item = match crate::eval::functions::dispatch_generator_method(
            state,
            &input,
            "__next__",
            &[],
            &empty,
            tools,
        )
        .await
        {
            Ok(v) => v,
            Err(EvalError::Exception(e)) if e.type_name == "StopIteration" => break,
            Err(e) => return Err(e),
        };
        if idx >= start && (idx - start) % step == 0 {
            out.push(item);
        }
        idx += 1;
    }
    Ok(Value::List(shared_list(out)))
}

/// Build the lazy-`islice` generator (see `islice_impl`). Returns `None` if the
/// synthesized body is not suspend-drivable.
#[expect(clippy::cast_possible_wrap, reason = "islice bounds are small, non-negative usizes")]
fn synthesize_islice(
    state: &mut crate::state::InterpreterState,
    input: &Value,
    start: usize,
    stop: Option<usize>,
    step: usize,
) -> Option<Value> {
    use rustpython_parser::ast::{
        self as ast, Constant, Expr as E, ExprBinOp, ExprBoolOp, ExprCompare, ExprConstant,
        ExprContext, ExprName, ExprYield, Identifier, Stmt, StmtAssign, StmtBreak, StmtExpr,
        StmtFor, StmtIf,
    };
    use rustpython_parser::text_size::TextRange;

    let r = TextRange::default();
    let name =
        |id: &str, ctx: ExprContext| E::Name(ExprName { id: Identifier::new(id), ctx, range: r });
    let int_c =
        |n: i64| E::Constant(ExprConstant { value: Constant::Int(n.into()), kind: None, range: r });
    let idx_load = || name(".idx", ExprContext::Load);
    let cmp = |left: E, op: ast::CmpOp, right: E| {
        E::Compare(ExprCompare {
            left: Box::new(left),
            ops: vec![op],
            comparators: vec![right],
            range: r,
        })
    };

    let mut for_body: Vec<Stmt> = Vec::new();
    // yield .item, guarded by the start/step selection unless it is trivially
    // true. The `stop` break comes AFTER the increment (below) so exactly `stop`
    // items are consumed — checking before the yield would over-pull the source
    // by one (observable via a source's side effects), unlike CPython.
    let yield_stmt = Stmt::Expr(StmtExpr {
        value: Box::new(E::Yield(ExprYield {
            value: Some(Box::new(name(".item", ExprContext::Load))),
            range: r,
        })),
        range: r,
    });
    if start == 0 && step == 1 {
        for_body.push(yield_stmt);
    } else {
        // (.idx - start) % step == 0
        let step_ok = cmp(
            E::BinOp(ExprBinOp {
                left: Box::new(E::BinOp(ExprBinOp {
                    left: Box::new(idx_load()),
                    op: ast::Operator::Sub,
                    right: Box::new(int_c(start as i64)),
                    range: r,
                })),
                op: ast::Operator::Mod,
                right: Box::new(int_c(step as i64)),
                range: r,
            }),
            ast::CmpOp::Eq,
            int_c(0),
        );
        let cond = E::BoolOp(ExprBoolOp {
            op: ast::BoolOp::And,
            values: vec![cmp(idx_load(), ast::CmpOp::GtE, int_c(start as i64)), step_ok],
            range: r,
        });
        for_body.push(Stmt::If(StmtIf {
            test: Box::new(cond),
            body: vec![yield_stmt],
            orelse: vec![],
            range: r,
        }));
    }
    // .idx = .idx + 1
    for_body.push(Stmt::Assign(StmtAssign {
        targets: vec![name(".idx", ExprContext::Store)],
        value: Box::new(E::BinOp(ExprBinOp {
            left: Box::new(idx_load()),
            op: ast::Operator::Add,
            right: Box::new(int_c(1)),
            range: r,
        })),
        type_comment: None,
        range: r,
    }));
    // if .idx >= stop: break  (after the increment — consume exactly `stop`).
    if let Some(stop_v) = stop {
        for_body.push(Stmt::If(StmtIf {
            test: Box::new(cmp(idx_load(), ast::CmpOp::GtE, int_c(stop_v as i64))),
            body: vec![Stmt::Break(StmtBreak { range: r })],
            orelse: vec![],
            range: r,
        }));
    }

    let body = std::sync::Arc::new(vec![
        Stmt::Assign(StmtAssign {
            targets: vec![name(".idx", ExprContext::Store)],
            value: Box::new(int_c(0)),
            type_comment: None,
            range: r,
        }),
        Stmt::For(StmtFor {
            target: Box::new(name(".item", ExprContext::Store)),
            iter: Box::new(name(".0", ExprContext::Load)),
            body: for_body,
            orelse: vec![],
            type_comment: None,
            range: r,
        }),
    ]);

    let mut locals: rustc_hash::FxHashMap<String, Value> = rustc_hash::FxHashMap::default();
    locals.insert(".0".to_string(), input.clone());
    let touched = vec![".0".to_string(), ".idx".to_string(), ".item".to_string()];
    crate::eval::functions::create_synthetic_generator(state, "<islice>", body, locals, touched)
}

/// `groupby(iterable, key=None)`: group consecutive elements sharing a key.
/// Yields `(key, group)` pairs; in our eager model the group materialises as a
/// list. `key=None` groups by the elements themselves. Only *consecutive* runs
/// group, matching CPython (callers `sorted()` first for a full grouping).
async fn groupby_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    kwargs: &indexmap::IndexMap<String, Value>,
    tools: &crate::tools::Tools,
) -> EvalResult {
    let items = crate::eval::op::iter(state, need_arg("groupby", args, 0)?, tools).await?;
    let key_fn =
        args.get(1).or_else(|| kwargs.get("key")).filter(|v| !matches!(v, Value::None)).cloned();
    let empty = indexmap::IndexMap::new();
    let mut out: Vec<Value> = Vec::new();
    let mut current: Option<(Value, Vec<Value>)> = None;
    for item in items {
        let key = match &key_fn {
            Some(f) => {
                crate::eval::modules::call_callable(
                    state,
                    f,
                    std::slice::from_ref(&item),
                    &empty,
                    tools,
                )
                .await?
            }
            None => item.clone(),
        };
        match &mut current {
            Some((ck, group)) if crate::eval::operations::values_equal_pub(ck, &key) => {
                group.push(item);
            }
            _ => {
                if let Some((ck, group)) = current.take() {
                    out.push(Value::Tuple(vec![ck, Value::List(shared_list(group))]));
                }
                current = Some((key, vec![item]));
            }
        }
    }
    if let Some((ck, group)) = current {
        out.push(Value::Tuple(vec![ck, Value::List(shared_list(group))]));
    }
    Ok(Value::List(shared_list(out)))
}

/// True for a lazy iterator value (a generator / lazy buffer / count-cycle-repeat).
fn is_lazy_iter(v: &Value) -> bool {
    matches!(v, Value::Generator { .. } | Value::Lazy { .. } | Value::BuiltinIter { .. })
}

/// Build a lazy itertools generator by parsing a Python body template and
/// binding its free parameters as frame locals — the body's `for x in <input>`
/// is then stepped one item at a time by the generator suspend engine. Returns
/// `None` if the body is not suspend-drivable (caller falls back to the eager
/// path). Reused so an infinite or closure-bearing lazy input is not
/// materialised (which would hang or capture the final loop value).
fn lazy_it_gen(
    state: &mut crate::state::InterpreterState,
    name: &str,
    body_src: &str,
    bindings: &[(&str, Value)],
) -> Option<Value> {
    let body = crate::parser::parse(body_src).ok()?;
    let (assigned, _globals) = crate::eval::functions::collect_assigned_names(&body);
    let mut locals: rustc_hash::FxHashMap<String, Value> = rustc_hash::FxHashMap::default();
    let mut touched: Vec<String> = Vec::new();
    for (n, v) in bindings {
        locals.insert((*n).to_string(), v.clone());
        touched.push((*n).to_string());
    }
    for n in assigned {
        if !touched.iter().any(|t| t == &n) {
            touched.push(n);
        }
    }
    crate::eval::functions::create_synthetic_generator(
        state,
        name,
        std::sync::Arc::new(body),
        locals,
        touched,
    )
}

async fn takewhile_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let pred = need_arg("takewhile", args, 0)?.clone();
    let iter_val = need_arg("takewhile", args, 1)?.clone();
    let kwargs = indexmap::IndexMap::new();
    let mut out = Vec::new();
    // A lazy input is stepped one item at a time (so an infinite source stops at
    // the predicate and loop-var closures capture their interleaved value).
    if is_lazy_iter(&iter_val) {
        if let Some(g) = lazy_it_gen(
            state,
            "<takewhile>",
            "for item in it:\n    if not pred(item):\n        break\n    yield item\n",
            &[("pred", pred.clone()), ("it", iter_val.clone())],
        ) {
            return Ok(g);
        }
    }
    for item in iterate_value(&iter_val)? {
        let verdict = crate::eval::modules::call_callable(
            state,
            &pred,
            std::slice::from_ref(&item),
            &kwargs,
            tools,
        )
        .await?;
        if !verdict.is_truthy() {
            break;
        }
        out.push(item);
    }
    Ok(Value::List(shared_list(out)))
}

/// `dropwhile(predicate, iterable)`: drop items while predicate returns
/// truthy; yield all remaining items unconditionally once the predicate
/// has fired False for the first time.
async fn dropwhile_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let pred = need_arg("dropwhile", args, 0)?.clone();
    let iter_val = need_arg("dropwhile", args, 1)?.clone();
    if is_lazy_iter(&iter_val) {
        if let Some(g) = lazy_it_gen(
            state,
            "<dropwhile>",
            "dropping = True\nfor item in it:\n    if dropping and pred(item):\n        continue\n    dropping = False\n    yield item\n",
            &[("pred", pred.clone()), ("it", iter_val.clone())],
        ) {
            return Ok(g);
        }
    }
    let items = crate::eval::op::iter(state, &iter_val, tools).await?;
    let kwargs = indexmap::IndexMap::new();
    let mut out = Vec::new();
    let mut dropping = true;
    for item in items {
        if dropping {
            let verdict = crate::eval::modules::call_callable(
                state,
                &pred,
                std::slice::from_ref(&item),
                &kwargs,
                tools,
            )
            .await?;
            if verdict.is_truthy() {
                continue;
            }
            dropping = false;
        }
        out.push(item);
    }
    Ok(Value::List(shared_list(out)))
}

/// `count(start=0, step=1)` — register an unbounded counter. `start`
/// and `step` must be numbers (CPython rejects non-numeric with
/// TypeError).
fn make_count(state: &mut crate::state::InterpreterState, args: &[Value]) -> EvalResult {
    let numeric = |v: &Value, what: &str| -> Result<Value, EvalError> {
        match v {
            Value::Int(_) | Value::BigInt(_) | Value::Float(_) | Value::Bool(_) => Ok(v.clone()),
            other => Err(InterpreterError::TypeError(format!(
                "a number is required for count() {what}, not '{}'",
                other.type_name()
            ))
            .into()),
        }
    };
    let start = match args.first() {
        None | Some(Value::None) => Value::Int(0),
        Some(v) => numeric(v, "start")?,
    };
    let step = match args.get(1) {
        None | Some(Value::None) => Value::Int(1),
        Some(v) => numeric(v, "step")?,
    };
    Ok(state.alloc_builtin_iter(
        crate::value::BuiltinIterName::Count,
        crate::state::BuiltinIterState::Count { next: start, step },
    ))
}

/// `cycle(iterable)` — buffer the (finite) input eagerly, then repeat it
/// forever.
fn make_cycle(state: &mut crate::state::InterpreterState, args: &[Value]) -> EvalResult {
    let items = iterate_value(need_arg("cycle", args, 0)?)?;
    Ok(state.alloc_builtin_iter(
        crate::value::BuiltinIterName::Cycle,
        crate::state::BuiltinIterState::Cycle { items, pos: 0 },
    ))
}

/// `starmap(function, iterable)`: call `function(*args)` for each
/// `args` tuple/iterable in the input.
async fn starmap_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let func = need_arg("starmap", args, 0)?.clone();
    let iter_val = need_arg("starmap", args, 1)?.clone();
    if is_lazy_iter(&iter_val) {
        if let Some(g) = lazy_it_gen(
            state,
            "<starmap>",
            "for item in it:\n    yield func(*item)\n",
            &[("func", func.clone()), ("it", iter_val.clone())],
        ) {
            return Ok(g);
        }
    }
    let kwargs = indexmap::IndexMap::new();
    let mut out = Vec::new();
    for item in crate::eval::op::iter(state, &iter_val, tools).await? {
        let call_args = iterate_value(&item)?;
        out.push(
            crate::eval::modules::call_callable(state, &func, &call_args, &kwargs, tools).await?,
        );
    }
    Ok(Value::List(shared_list(out)))
}

/// `filterfalse(predicate, iterable)`: keep items where `predicate` is
/// falsy (`predicate=None` keeps falsy items directly).
async fn filterfalse_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    tools: &crate::tools::Tools,
) -> EvalResult {
    let pred = need_arg("filterfalse", args, 0)?.clone();
    let iter_val = need_arg("filterfalse", args, 1)?.clone();
    if is_lazy_iter(&iter_val) {
        if let Some(g) = lazy_it_gen(
            state,
            "<filterfalse>",
            "for item in it:\n    if (not item) if pred is None else (not pred(item)):\n        yield item\n",
            &[("pred", pred.clone()), ("it", iter_val.clone())],
        ) {
            return Ok(g);
        }
    }
    let kwargs = indexmap::IndexMap::new();
    let mut out = Vec::new();
    for item in crate::eval::op::iter(state, &iter_val, tools).await? {
        let keep = if matches!(pred, Value::None) {
            !item.is_truthy()
        } else {
            !crate::eval::modules::call_callable(
                state,
                &pred,
                std::slice::from_ref(&item),
                &kwargs,
                tools,
            )
            .await?
            .is_truthy()
        };
        if keep {
            out.push(item);
        }
    }
    Ok(Value::List(shared_list(out)))
}

/// `accumulate(iterable, [func=operator.add])`: cumulative reductions.
/// Default reducer is addition; passing a callable folds via it.
async fn accumulate_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    call_kwargs: &indexmap::IndexMap<String, Value>,
    tools: &crate::tools::Tools,
) -> EvalResult {
    let iter_val = need_arg("accumulate", args, 0)?;
    let reducer = args.get(1).cloned();
    if is_lazy_iter(iter_val) {
        let func = reducer.clone().unwrap_or(Value::None);
        let initial = call_kwargs.get("initial").cloned().unwrap_or(Value::None);
        if let Some(g) = lazy_it_gen(
            state,
            "<accumulate>",
            "first = True\nacc = None\nif initial is not None:\n    acc = initial\n    first = False\n    yield acc\nfor item in it:\n    if first:\n        acc = item\n        first = False\n    else:\n        acc = (acc + item) if func is None else func(acc, item)\n    yield acc\n",
            &[("it", iter_val.clone()), ("func", func), ("initial", initial)],
        ) {
            return Ok(g);
        }
    }
    let items = iterate_value(iter_val)?;
    let kwargs = indexmap::IndexMap::new();
    let mut out: Vec<Value> = Vec::new();
    // `initial=`: seed the accumulator and emit it first, so the output has one
    // more element than the input (CPython 3.8+).
    let mut acc: Option<Value> = match call_kwargs.get("initial") {
        None | Some(Value::None) => None,
        Some(seed) => {
            out.push(seed.clone());
            Some(seed.clone())
        }
    };
    for item in items {
        acc = Some(match acc {
            None => item,
            Some(prev) => match &reducer {
                Some(callable) => {
                    crate::eval::modules::call_callable(
                        state,
                        callable,
                        &[prev, item],
                        &kwargs,
                        tools,
                    )
                    .await?
                }
                None => crate::types::dispatch_binop(
                    crate::types::BinOp::Add,
                    &prev,
                    &item,
                    state.decimal_prec,
                )?,
            },
        });
        if let Some(v) = &acc {
            out.push(v.clone());
        }
    }
    Ok(Value::List(shared_list(out)))
}

/// `pairwise(iterable)` — overlapping pairs, lazy over a lazy input.
async fn pairwise_impl(
    state: &mut crate::state::InterpreterState,
    args: &[Value],
    _tools: &crate::tools::Tools,
) -> EvalResult {
    let iter_val = need_arg("pairwise", args, 0)?.clone();
    if is_lazy_iter(&iter_val) {
        if let Some(g) = lazy_it_gen(
            state,
            "<pairwise>",
            "prev = None\nhas_prev = False\nfor item in it:\n    if has_prev:\n        yield (prev, item)\n    prev = item\n    has_prev = True\n",
            &[("it", iter_val.clone())],
        ) {
            return Ok(g);
        }
    }
    let items = iterate_value(&iter_val)?;
    let out: Vec<Value> =
        items.windows(2).map(|w| Value::Tuple(vec![w[0].clone(), w[1].clone()])).collect();
    Ok(Value::List(shared_list(out)))
}

/// `compress(data, selectors)` — yield data items where the selector is truthy;
/// lazy when either input is lazy so it does not drain an infinite source.
async fn compress_lazy(state: &mut crate::state::InterpreterState, args: &[Value]) -> EvalResult {
    let data = need_arg("compress", args, 0)?.clone();
    let selectors = need_arg("compress", args, 1)?.clone();
    if is_lazy_iter(&data) || is_lazy_iter(&selectors) {
        if let Some(g) = lazy_it_gen(
            state,
            "<compress>",
            "_missing = object()\nsit = iter(selectors)\nfor item in data:\n    s = next(sit, _missing)\n    if s is _missing:\n        break\n    if s:\n        yield item\n",
            &[("data", data.clone()), ("selectors", selectors.clone())],
        ) {
            return Ok(g);
        }
    }
    compress_impl(args)
}

/// `itertools` module registration. Predicate-driven functions
/// (`takewhile`, `dropwhile`, `accumulate`) re-enter the evaluator to
/// invoke the user-supplied callable; eager ones (`chain`, `repeat`,
/// `combinations`, ...) route to the sync `call` body.
pub struct ItertoolsModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for ItertoolsModule {
    fn name(&self) -> &'static str {
        "itertools"
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        kwargs: &indexmap::IndexMap<String, Value>,
        tools: &crate::tools::Tools,
    ) -> EvalResult {
        let result = match func {
            "takewhile" => takewhile_impl(state, args, tools).await,
            "dropwhile" => dropwhile_impl(state, args, tools).await,
            "accumulate" => accumulate_impl(state, args, kwargs, tools).await,
            "compress" => compress_lazy(state, args).await,
            "pairwise" => pairwise_impl(state, args, tools).await,
            "groupby" => groupby_impl(state, args, kwargs, tools).await,
            "islice" => islice_impl(state, args, tools).await,
            "starmap" => starmap_impl(state, args, tools).await,
            "filterfalse" => filterfalse_impl(state, args, tools).await,
            "count" => make_count(state, args),
            "cycle" => make_cycle(state, args),
            // `repeat(obj)` with no count is the unbounded form (the
            // counted form stays eager in the sync `call`).
            "repeat" if args.len() < 2 => Ok(state.alloc_builtin_iter(
                crate::value::BuiltinIterName::Repeat,
                crate::state::BuiltinIterState::Repeat {
                    value: need_arg("repeat", args, 0)?.clone(),
                },
            )),
            _ => call(func, args, kwargs),
        }?;
        Ok(lazy_wrap(state, func, result))
    }
}

/// itertools producers are single-use iterators, not lists. Wrap the eagerly
/// built `List` result in the `Lazy` cursor type so `next()`, single-pass
/// iteration, and non-subscriptability match CPython. `tee` returns a tuple of
/// independent iterators, so each element is wrapped. Results that are already
/// iterators (`count`/`cycle`/`repeat`'s `BuiltinIter`) pass through unchanged.
fn lazy_wrap(state: &mut crate::state::InterpreterState, func: &str, value: Value) -> Value {
    match value {
        Value::List(items) => state.alloc_lazy(items.lock().clone()),
        Value::Tuple(copies) if func == "tee" => Value::Tuple(
            copies
                .into_iter()
                .map(|c| match c {
                    Value::List(items) => state.alloc_lazy(items.lock().clone()),
                    other => other,
                })
                .collect(),
        ),
        other => other,
    }
}
