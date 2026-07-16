// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use rustpython_parser::ast::{self, Expr};

use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::{
        eval_expr,
        functions::{VariableCheckpoint, resolve_proxy},
        literals::value_to_key,
    },
    state::InterpreterState,
    tools::Tools,
    value::{Value, ValueKey, shared_list},
};

/// Collect every name introduced by a comprehension's `for X in ...`
/// generator targets (e.g. `x, y` in `[... for x, y in pairs]`). These
/// names are scoped to the comprehension in CPython; in our flat-state
/// model we snapshot their pre-comp values and restore on exit so the
/// names don't leak. Walrus targets are deliberately NOT collected
/// here — PEP 572 binds them to the comprehension's *enclosing* scope.
fn collect_generator_target_names(generators: &[ast::Comprehension]) -> Vec<String> {
    let mut names = Vec::new();
    for g in generators {
        collect_target_names(&g.target, &mut names);
    }
    names
}

/// Same `collect_target_names` walker the function-frame checkpoint uses,
/// inlined here so the comprehension module doesn't need a public export
/// from the `functions` module. Kept private — this isn't a stable API.
fn collect_target_names(target: &ast::Expr, out: &mut Vec<String>) {
    match target {
        Expr::Name(n) => {
            let s = n.id.as_str().to_string();
            if !out.contains(&s) {
                out.push(s);
            }
        }
        Expr::Tuple(t) => {
            for e in &t.elts {
                collect_target_names(e, out);
            }
        }
        Expr::List(l) => {
            for e in &l.elts {
                collect_target_names(e, out);
            }
        }
        Expr::Starred(s) => collect_target_names(&s.value, out),
        _ => {}
    }
}

/// Collect the target names of every walrus (`:=`) that binds directly in
/// this comprehension's scope, i.e. appears in the element/key/value or a
/// filter `if` clause. Nested comprehensions and lambdas open their own
/// scopes, so the walk stops at them (their walruses are validated when they
/// are evaluated). The `iter` of a generator is deliberately excluded — it is
/// evaluated in the enclosing scope, where a walrus is legal.
fn collect_scope_walrus_targets(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::NamedExpr(n) => {
            if let Expr::Name(name) = n.target.as_ref() {
                let s = name.id.as_str().to_string();
                if !out.contains(&s) {
                    out.push(s);
                }
            }
            collect_scope_walrus_targets(&n.value, out);
        }
        Expr::BoolOp(b) => b.values.iter().for_each(|e| collect_scope_walrus_targets(e, out)),
        Expr::BinOp(b) => {
            collect_scope_walrus_targets(&b.left, out);
            collect_scope_walrus_targets(&b.right, out);
        }
        Expr::UnaryOp(u) => collect_scope_walrus_targets(&u.operand, out),
        Expr::Compare(c) => {
            collect_scope_walrus_targets(&c.left, out);
            c.comparators.iter().for_each(|e| collect_scope_walrus_targets(e, out));
        }
        Expr::IfExp(i) => {
            collect_scope_walrus_targets(&i.test, out);
            collect_scope_walrus_targets(&i.body, out);
            collect_scope_walrus_targets(&i.orelse, out);
        }
        Expr::Call(c) => {
            collect_scope_walrus_targets(&c.func, out);
            c.args.iter().for_each(|e| collect_scope_walrus_targets(e, out));
            c.keywords.iter().for_each(|k| collect_scope_walrus_targets(&k.value, out));
        }
        Expr::Subscript(s) => {
            collect_scope_walrus_targets(&s.value, out);
            collect_scope_walrus_targets(&s.slice, out);
        }
        Expr::Slice(s) => {
            for part in [&s.lower, &s.upper, &s.step].into_iter().flatten() {
                collect_scope_walrus_targets(part, out);
            }
        }
        Expr::Attribute(a) => collect_scope_walrus_targets(&a.value, out),
        Expr::Starred(s) => collect_scope_walrus_targets(&s.value, out),
        Expr::Await(a) => collect_scope_walrus_targets(&a.value, out),
        Expr::Tuple(t) => t.elts.iter().for_each(|e| collect_scope_walrus_targets(e, out)),
        Expr::List(l) => l.elts.iter().for_each(|e| collect_scope_walrus_targets(e, out)),
        Expr::Set(s) => s.elts.iter().for_each(|e| collect_scope_walrus_targets(e, out)),
        Expr::Dict(d) => {
            d.keys.iter().flatten().for_each(|e| collect_scope_walrus_targets(e, out));
            d.values.iter().for_each(|e| collect_scope_walrus_targets(e, out));
        }
        Expr::FormattedValue(f) => collect_scope_walrus_targets(&f.value, out),
        Expr::JoinedStr(j) => j.values.iter().for_each(|e| collect_scope_walrus_targets(e, out)),
        // A nested comprehension or lambda opens a new scope: its walruses are
        // validated when it is evaluated, so do not descend.
        _ => {}
    }
}

/// PEP 572 / CPython symtable: a walrus target inside a comprehension may not
/// rebind one of that comprehension's `for` iteration variables. CPython
/// rejects this at compile time with a SyntaxError; we raise the same error
/// when the comprehension is evaluated.
fn check_walrus_rebind(
    generators: &[ast::Comprehension],
    body_exprs: &[&Expr],
) -> Result<(), EvalError> {
    let iter_vars = collect_generator_target_names(generators);
    if iter_vars.is_empty() {
        return Ok(());
    }
    let mut walrus_targets = Vec::new();
    for e in body_exprs {
        collect_scope_walrus_targets(e, &mut walrus_targets);
    }
    for g in generators {
        for cond in &g.ifs {
            collect_scope_walrus_targets(cond, &mut walrus_targets);
        }
    }
    if let Some(clash) = walrus_targets.iter().find(|t| iter_vars.contains(t)) {
        return Err(InterpreterError::Syntax(format!(
            "assignment expression cannot rebind comprehension iteration variable '{clash}'"
        ))
        .into());
    }
    Ok(())
}

/// Evaluate a list comprehension [expr for x in iterable if cond].
///
/// The comprehension target names are scoped to the comprehension —
/// snapshotted on entry and restored on exit. Walrus targets inside
/// the comprehension are deliberately NOT included in the checkpoint,
/// so they propagate to the enclosing function scope (PEP 572).
pub async fn eval_list_comp(
    state: &mut InterpreterState,
    node: &ast::ExprListComp,
    tools: &Tools,
) -> EvalResult {
    check_walrus_rebind(&node.generators, &[&node.elt])?;
    let checkpoint =
        VariableCheckpoint::capture(state, collect_generator_target_names(&node.generators));
    let mut results = Vec::new();

    let outcome = eval_list_generators(ListGenContext {
        state,
        generators: &node.generators,
        index: 0,
        elt_expr: &node.elt,
        results: &mut results,
        tools,
    })
    .await;

    checkpoint.restore(state);
    outcome?;

    Ok(Value::List(shared_list(results)))
}

/// Evaluate a generator expression `(expr for x in iterable if cond)`.
///
/// The interpreter has no coroutine/`yield` machinery, so a generator is
/// materialised eagerly into a `Value::List`. Every consumer in this sandbox
/// (`sum`/`all`/`any`/`min`/`max`/`sorted`/`list`/`set` and `for` loops) treats
/// the result as a plain iterable, so eager materialisation is observably
/// identical to lazy iteration for bounded inputs — and the operation/loop
/// limits already bound the input. The cost is that side effects run at
/// construction time rather than on demand and that unbounded generators are
/// not representable; both are acceptable in a sandbox that forbids I/O and
/// caps iteration counts.
pub async fn eval_generator_exp(
    state: &mut InterpreterState,
    node: &ast::ExprGeneratorExp,
    tools: &Tools,
) -> EvalResult {
    check_walrus_rebind(&node.generators, &[&node.elt])?;

    // A single-generator genexp is evaluated *lazily* (through the real suspend
    // engine) in two cases:
    //   1. the element builds a closure — a closure yielded mid-iteration must
    //      be callable while the loop variable still holds its yield-time value
    //      (CPython's `[f() for f in (lambda: k for k in range(n))]` → `[0,1,2]`;
    //      eager materialisation would run the loop to the end first);
    //   2. the source is itself a lazy/infinite iterator (a generator, another
    //      genexp, `itertools.count()`, …) — eager materialisation would drain
    //      or hang on it, so `next(x*x for x in count())` must stay lazy.
    // Every other genexp (finite, materialisable source, non-closure element)
    // keeps the proven eager path — its result is a `Lazy` buffer that the sync
    // consumers (`str.join`, `dict()`, tuple-unpack) can still step.
    if node.generators.len() == 1 && !node.generators[0].is_async {
        if let Some(generator) = try_lazy_genexp(state, node, tools).await? {
            return Ok(generator);
        }
    }

    let checkpoint =
        VariableCheckpoint::capture(state, collect_generator_target_names(&node.generators));
    let mut results = Vec::new();

    let outcome = eval_list_generators(ListGenContext {
        state,
        generators: &node.generators,
        index: 0,
        elt_expr: &node.elt,
        results: &mut results,
        tools,
    })
    .await;

    checkpoint.restore(state);
    outcome?;

    // A generator expression is a one-shot lazy iterator, not a list: `next(g)`
    // advances it and a later `list(g)` yields only the remainder. We eagerly
    // materialise the items (the sandbox caps iteration and forbids unbounded
    // streams) but wrap them in the `Lazy` cursor type so the iterator protocol
    // (`next`, single-pass `for`/`list`/`sum`) behaves as CPython's does.
    let cursor_id = state.next_cursor_id;
    state.next_cursor_id = state.next_cursor_id.wrapping_add(1);
    state.lazy_cursors.insert(cursor_id, 0);
    Ok(Value::Lazy { items: results, cursor_id })
}

/// Whether an expression constructs a closure (contains a `lambda`), so a
/// generator expression yielding it needs lazy evaluation. Walks the common
/// expression forms; a missed exotic form just keeps the eager path (safe).
fn expr_builds_closure(expr: &Expr) -> bool {
    match expr {
        Expr::Lambda(_) => true,
        Expr::BoolOp(n) => n.values.iter().any(expr_builds_closure),
        Expr::BinOp(n) => expr_builds_closure(&n.left) || expr_builds_closure(&n.right),
        Expr::UnaryOp(n) => expr_builds_closure(&n.operand),
        Expr::IfExp(n) => {
            expr_builds_closure(&n.test)
                || expr_builds_closure(&n.body)
                || expr_builds_closure(&n.orelse)
        }
        Expr::Tuple(n) => n.elts.iter().any(expr_builds_closure),
        Expr::List(n) => n.elts.iter().any(expr_builds_closure),
        Expr::Set(n) => n.elts.iter().any(expr_builds_closure),
        Expr::Call(n) => {
            expr_builds_closure(&n.func)
                || n.args.iter().any(expr_builds_closure)
                || n.keywords.iter().any(|k| expr_builds_closure(&k.value))
        }
        Expr::Dict(n) => {
            n.keys.iter().flatten().any(expr_builds_closure)
                || n.values.iter().any(expr_builds_closure)
        }
        _ => false,
    }
}

/// Lazily evaluate a single-generator `genexp` by synthesising a real
/// generator function (`for <target> in .0: [if cond:]* yield <elt>`) and
/// routing it through the suspend engine. The first iterable is evaluated
/// eagerly (CPython) and bound to the hidden local `.0`; the element's free
/// names are captured into the generator frame. Returns `None` (fall back to
/// the eager path) when the body would not be suspendable.
async fn try_lazy_genexp(
    state: &mut InterpreterState,
    node: &ast::ExprGeneratorExp,
    tools: &Tools,
) -> Result<Option<Value>, EvalError> {
    use rustpython_parser::ast::{
        Expr as E, ExprContext, ExprName, ExprYield, Identifier, Stmt, StmtExpr, StmtFor, StmtIf,
    };
    use rustpython_parser::text_size::TextRange;
    let comp = &node.generators[0];

    // Evaluate the outermost iterable eagerly (CPython) and stash it under the
    // hidden `.0` local — an invalid identifier, so no user name collides.
    let iter_value = resolve_proxy(&eval_expr(state, &comp.iter, tools).await?).await?;

    // Only take the lazy path when it is actually needed: a closure-building
    // element (loop-variable capture timing) or a lazy/infinite source (a
    // generator / another genexp / `count()`), which the eager path would drain
    // or hang on. A finite, materialisable source with a plain element stays
    // eager so its `Lazy` result remains steppable by sync consumers
    // (`str.join`, `dict()`). Falling back re-evaluates the source on the eager
    // path — the same double-evaluation the closure path already accepted.
    let source_is_lazy = matches!(
        iter_value,
        Value::Generator { .. } | Value::Lazy { .. } | Value::BuiltinIter { .. }
    );
    if !source_is_lazy && !expr_builds_closure(&node.elt) {
        return Ok(None);
    }

    // Build `for <target> in .0: [if cond:]* yield <elt>`.
    let iter_ref = E::Name(ExprName {
        id: Identifier::new(".0"),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    });
    let mut inner: Stmt = Stmt::Expr(StmtExpr {
        value: Box::new(E::Yield(ExprYield {
            value: Some(Box::new(node.elt.as_ref().clone())),
            range: TextRange::default(),
        })),
        range: TextRange::default(),
    });
    for cond in comp.ifs.iter().rev() {
        inner = Stmt::If(StmtIf {
            test: Box::new(cond.clone()),
            body: vec![inner],
            orelse: vec![],
            range: TextRange::default(),
        });
    }
    let for_stmt = Stmt::For(StmtFor {
        target: Box::new(comp.target.clone()),
        iter: Box::new(iter_ref),
        body: vec![inner],
        orelse: vec![],
        type_comment: None,
        range: TextRange::default(),
    });
    let body = std::sync::Arc::new(vec![for_stmt]);

    // The body must be re-enterable by the suspend engine; otherwise fall back.
    if !crate::eval::functions::generator_suspendable(&body) {
        return Ok(None);
    }

    // Capture the element's free names (everything read minus the loop targets
    // and the synthetic `.0`) from the enclosing scope into the frame locals.
    let target_names = collect_generator_target_names(&node.generators);
    let free = crate::eval::functions::collect_free_names(&target_names, body.as_slice());
    let mut locals: rustc_hash::FxHashMap<String, Value> = rustc_hash::FxHashMap::default();
    let mut touched: Vec<String> = Vec::new();
    for name in &free {
        if name == ".0" {
            continue;
        }
        if let Some(v) = state.variables.get(name) {
            locals.insert(name.clone(), v.clone());
        }
        touched.push(name.clone());
    }
    locals.insert(".0".to_string(), iter_value);
    touched.push(".0".to_string());
    touched.extend(target_names);

    let func_def = crate::value::FunctionDef {
        name: "<genexpr>".to_string(),
        body_key: format!("<genexpr>#{}", state.next_cursor_id),
        wraps_name: None,
        params: crate::value::FunctionParams {
            args: Vec::new(),
            defaults: Vec::new(),
            default_values: Vec::new(),
            vararg: None,
            kwonlyargs: Vec::new(),
            kw_defaults: Vec::new(),
            kw_default_values: Vec::new(),
            kwarg: None,
            posonly_count: 0,
        },
        closure: std::collections::BTreeMap::new(),
        source: String::new(),
        nonlocal_names: Vec::new(),
        is_generator: true,
        nonlocal_cell_id: None,
        assigned_names: Vec::new(),
        global_names: Vec::new(),
        is_module_level: state.call_depth == 0,
        docstring: None,
        cell_refreshes: Vec::new(),
        qualname: state.qualname_for("<genexpr>"),
    };
    let generator =
        crate::eval::functions::create_generator(state, &func_def, body, locals, touched);
    Ok(Some(generator))
}

/// Evaluate a dict comprehension {key: val for x in iterable if cond}.
pub async fn eval_dict_comp(
    state: &mut InterpreterState,
    node: &ast::ExprDictComp,
    tools: &Tools,
) -> EvalResult {
    check_walrus_rebind(&node.generators, &[&node.key, &node.value])?;
    let checkpoint =
        VariableCheckpoint::capture(state, collect_generator_target_names(&node.generators));
    let mut result_map = indexmap::IndexMap::new();

    let outcome = eval_dict_generators(DictGenContext {
        state,
        generators: &node.generators,
        index: 0,
        key_expr: &node.key,
        value_expr: &node.value,
        result_map: &mut result_map,
        tools,
    })
    .await;

    checkpoint.restore(state);
    outcome?;

    Ok(Value::Dict(crate::value::shared_dict(result_map)))
}

/// Evaluate a set comprehension {expr for x in iterable if cond}.
pub async fn eval_set_comp(
    state: &mut InterpreterState,
    node: &ast::ExprSetComp,
    tools: &Tools,
) -> EvalResult {
    check_walrus_rebind(&node.generators, &[&node.elt])?;
    let checkpoint =
        VariableCheckpoint::capture(state, collect_generator_target_names(&node.generators));
    let mut results = Vec::new();

    let outcome = eval_list_generators(ListGenContext {
        state,
        generators: &node.generators,
        index: 0,
        elt_expr: &node.elt,
        results: &mut results,
        tools,
    })
    .await;

    checkpoint.restore(state);
    outcome?;

    // Shared set construction: raises on an unhashable element and dedups
    // instances by __eq__. The old open-coded `value_to_key(x).ok()` dedup
    // silently dropped every element after the first unhashable one (all
    // compared equal as `None`), losing data instead of raising.
    crate::eval::literals::build_set(state, results, false, tools).await
}

/// Per-call context for [`eval_list_generators`].
struct ListGenContext<'a> {
    state: &'a mut InterpreterState,
    generators: &'a [ast::Comprehension],
    index: usize,
    elt_expr: &'a Expr,
    results: &'a mut Vec<Value>,
    tools: &'a Tools,
}

/// Recursively evaluate generators for list/set comprehensions.
fn eval_list_generators<'a>(
    ctx: ListGenContext<'a>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EvalError>> + Send + 'a>> {
    Box::pin(async move {
        let ListGenContext { state, generators, index, elt_expr, results, tools } = ctx;

        if index >= generators.len() {
            // Base case: evaluate the element expression
            let val = eval_expr(state, elt_expr, tools).await?;
            results.push(val);
            return Ok(());
        }

        let generator = &generators[index];
        let iterable = eval_expr(state, &generator.iter, tools).await?;
        let iterable = resolve_proxy(&iterable).await?;

        // Range fast path: walk (start, stop, step) without materializing
        // 10k Value::Int entries the iter consumer would immediately drop.
        // `[x * x for x in range(10000)]` is the canonical case.
        if let Value::Range { start, stop, step } = iterable {
            let pos = step > 0;
            let mut i = start;
            loop {
                let in_range = (pos && i < stop) || (step < 0 && i > stop);
                if !in_range {
                    break;
                }
                set_comprehension_target(state, &generator.target, &Value::Int(i)).await?;

                let mut include = true;
                for if_clause in &generator.ifs {
                    let cond = eval_expr(state, if_clause, tools).await?;
                    let cond = resolve_proxy(&cond).await?;
                    if !crate::eval::op::truthy(state, &cond, tools).await? {
                        include = false;
                        break;
                    }
                }
                if include {
                    eval_list_generators(ListGenContext {
                        state,
                        generators,
                        index: index + 1,
                        elt_expr,
                        results,
                        tools,
                    })
                    .await?;
                }

                let Some(next) = i.checked_add(step) else { break };
                i = next;
            }
            return Ok(());
        }

        // Lazy generator / iterator source: step it one item at a time so a
        // closure yielded mid-iteration (`[f() for f in (lambda: k for k in
        // range(n))]`) is processed while the loop variable still holds its
        // yield-time value — matching CPython, and mirroring the `for` loop's
        // lazy stepping. Other iterables materialise as before.
        if matches!(iterable, Value::Generator { .. } | Value::BuiltinIter { .. }) {
            let empty = indexmap::IndexMap::new();
            loop {
                let item = match crate::eval::functions::dispatch_generator_method(
                    state,
                    &iterable,
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
                set_comprehension_target(state, &generator.target, &item).await?;
                let mut include = true;
                for if_clause in &generator.ifs {
                    let cond = resolve_proxy(&eval_expr(state, if_clause, tools).await?).await?;
                    if !crate::eval::op::truthy(state, &cond, tools).await? {
                        include = false;
                        break;
                    }
                }
                if include {
                    eval_list_generators(ListGenContext {
                        state,
                        generators,
                        index: index + 1,
                        elt_expr,
                        results,
                        tools,
                    })
                    .await?;
                }
            }
            return Ok(());
        }

        let items = crate::eval::op::iter(state, &iterable, tools).await?;

        for item in items {
            // Set the target variable
            set_comprehension_target(state, &generator.target, &item).await?;

            // Check if-filters
            let mut include = true;
            for if_clause in &generator.ifs {
                let cond = eval_expr(state, if_clause, tools).await?;
                let cond = resolve_proxy(&cond).await?;
                if !crate::eval::op::truthy(state, &cond, tools).await? {
                    include = false;
                    break;
                }
            }

            if include {
                eval_list_generators(ListGenContext {
                    state,
                    generators,
                    index: index + 1,
                    elt_expr,
                    results,
                    tools,
                })
                .await?;
            }
        }

        Ok(())
    })
}

/// Per-call context for [`eval_dict_generators`].
///
/// `key_expr` and `value_expr` are both `&Expr`; without bundling, a
/// silent transposition would flip every comprehension's key/value
/// pair. The struct makes the role of each `Expr` named at every
/// recursive call site.
struct DictGenContext<'a> {
    state: &'a mut InterpreterState,
    generators: &'a [ast::Comprehension],
    index: usize,
    key_expr: &'a Expr,
    value_expr: &'a Expr,
    result_map: &'a mut indexmap::IndexMap<ValueKey, Value>,
    tools: &'a Tools,
}

/// Recursively evaluate generators for dict comprehensions.
fn eval_dict_generators<'a>(
    ctx: DictGenContext<'a>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EvalError>> + Send + 'a>> {
    Box::pin(async move {
        let DictGenContext { state, generators, index, key_expr, value_expr, result_map, tools } =
            ctx;

        if index >= generators.len() {
            let key = eval_expr(state, key_expr, tools).await?;
            let value = eval_expr(state, value_expr, tools).await?;
            // Instance keys go through the async hash/`__eq__` protocol
            // (same as a dict literal); other keys use the sync
            // `value_to_key`. Previously the comprehension always called
            // `value_to_key`, which rejects instances as unhashable.
            if matches!(key, Value::Instance(_)) {
                crate::eval::op::dict_insert_instance_key_pub(
                    state, result_map, &key, value, tools,
                )
                .await?;
            } else {
                result_map.insert(value_to_key(&key)?, value);
            }
            return Ok(());
        }

        let generator = &generators[index];
        let iterable = eval_expr(state, &generator.iter, tools).await?;
        let iterable = resolve_proxy(&iterable).await?;

        // Range fast path: same rationale as the list-comp variant above.
        if let Value::Range { start, stop, step } = iterable {
            let pos = step > 0;
            let mut i = start;
            loop {
                let in_range = (pos && i < stop) || (step < 0 && i > stop);
                if !in_range {
                    break;
                }
                set_comprehension_target(state, &generator.target, &Value::Int(i)).await?;

                let mut include = true;
                for if_clause in &generator.ifs {
                    let cond = eval_expr(state, if_clause, tools).await?;
                    let cond = resolve_proxy(&cond).await?;
                    if !crate::eval::op::truthy(state, &cond, tools).await? {
                        include = false;
                        break;
                    }
                }
                if include {
                    eval_dict_generators(DictGenContext {
                        state,
                        generators,
                        index: index + 1,
                        key_expr,
                        value_expr,
                        result_map,
                        tools,
                    })
                    .await?;
                }

                let Some(next) = i.checked_add(step) else { break };
                i = next;
            }
            return Ok(());
        }

        let items = crate::eval::op::iter(state, &iterable, tools).await?;

        for item in items {
            set_comprehension_target(state, &generator.target, &item).await?;

            let mut include = true;
            for if_clause in &generator.ifs {
                let cond = eval_expr(state, if_clause, tools).await?;
                let cond = resolve_proxy(&cond).await?;
                if !crate::eval::op::truthy(state, &cond, tools).await? {
                    include = false;
                    break;
                }
            }

            if include {
                eval_dict_generators(DictGenContext {
                    state,
                    generators,
                    index: index + 1,
                    key_expr,
                    value_expr,
                    result_map,
                    tools,
                })
                .await?;
            }
        }

        Ok(())
    })
}

/// Set a comprehension target variable (handles simple names and tuple unpacking).
fn set_comprehension_target<'a>(
    state: &'a mut InterpreterState,
    target: &'a Expr,
    value: &'a Value,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EvalError>> + Send + 'a>> {
    Box::pin(async move {
        match target {
            Expr::Name(name_node) => {
                // Comprehension targets are comp-scoped (Python 3
                // semantics) and overwritten every iteration. The full
                // `set_variable` path runs memory accounting (estimate
                // size of old + new value) that's pure overhead per
                // element, so a direct map insert is used. But a
                // late-binding closure over the comp variable
                // (`[lambda: i for i in range(n)]`) backs it with a
                // capture cell owned by the enclosing frame, and every
                // iteration must write through so the shared cell ends
                // at the final value — otherwise the closures read the
                // seed. Do just that targeted write-through here.
                let name = name_node.id.as_str();
                if let Some(&cell_id) =
                    state.frame_cell_owners.last().and_then(|owners| owners.get(name))
                {
                    state
                        .nonlocal_cells
                        .entry(cell_id)
                        .or_default()
                        .insert(name.to_string(), value.clone());
                }
                state.variables.insert(name.to_string(), value.clone());
                Ok(())
            }
            Expr::Tuple(tuple_node) => {
                let items: Vec<Value> = match value {
                    Value::List(items) => items.lock().clone(),
                    Value::Tuple(items) => items.clone(),
                    Value::String(s) => {
                        s.chars().map(|c| Value::String(c.to_string().into())).collect()
                    }
                    _ => {
                        return Err(InterpreterError::TypeError(
                            "cannot unpack non-iterable value in comprehension".into(),
                        )
                        .into());
                    }
                };

                if tuple_node.elts.len() != items.len() {
                    return Err(InterpreterError::Runtime(
                        "cannot unpack tuple of wrong size in comprehension".into(),
                    )
                    .into());
                }

                for (elem, val) in tuple_node.elts.iter().zip(items.iter()) {
                    set_comprehension_target(state, elem, val).await?;
                }
                Ok(())
            }
            _ => Err(InterpreterError::Runtime(format!(
                "unsupported comprehension target (see CONFORMANCE.md#unsupported-language-features): {:?}",
                std::mem::discriminant(target)
            ))
            .into()),
        }
    })
}
