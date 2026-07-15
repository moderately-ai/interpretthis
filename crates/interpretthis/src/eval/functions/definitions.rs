// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use rustpython_parser::ast::{self};

use super::params::evaluate_param_defaults;
use crate::{
    error::{EvalError, EvalResult, InterpreterError},
    eval::eval_expr,
    state::InterpreterState,
    tools::Tools,
    value::{FunctionDef, FunctionParams, LambdaDef, Param, Value},
};

// ---------------------------------------------------------------------------
// FunctionDef
// ---------------------------------------------------------------------------

/// Evaluate a function definition — store it in state, capturing closure.
///
/// Track B2: applies function-level decorators in CPython's bottom-up
/// order. Each decorator is evaluated as an expression, then called
/// with the (possibly already-decorated) function as its single
/// argument; the result is rebound to the function's name in scope.
/// `@functools.wraps`-style metadata transfer is not modelled —
/// decorators that depend on it still run but don't propagate names.
pub async fn eval_function_def(
    state: &mut InterpreterState,
    node: &ast::StmtFunctionDef,
    tools: &Tools,
) -> EvalResult {
    let name = node.name.as_str();

    // Block dangerous builtin names
    crate::security::validator::validate_name(
        crate::security::validator::NameContext::FunctionDefinition,
        name,
    )?;

    // Block redefining tools
    if tools.contains_key(name) {
        return Err(InterpreterError::Security(format!(
            "'{name}' is not allowed to be overridden"
        ))
        .into());
    }
    // Build parameter spec
    let mut params = build_function_params(&node.args)?;

    // Evaluate default argument expressions at def time and stash
    // the values on `params`. CPython evaluates defaults once at
    // def time — the same value object is reused per call. Two
    // patterns depend on this:
    //   (a) the canonical `i=i` loop-capture idiom that pins the
    //       enclosing-scope `i` onto the lambda/def at def time;
    //   (b) the mutable-default gotcha — `def f(x=[])` shares the
    //       same list across calls.
    // Re-evaluating each call (the prior behaviour) silently
    // breaks both: (a) errors with NameError once the enclosing
    // scope has cleared; (b) erases the gotcha (sandbox-safer but
    // breaks libraries that rely on it).
    evaluate_param_defaults(state, &mut params, tools).await?;

    // Populate the parse cache under a unique key so two same-named nested
    // functions in different scopes (`make1`/`make2` each with a `helper`)
    // don't overwrite each other's cached body.
    let body_key = format!("{name}#{}", state.next_cursor_id);
    state.next_cursor_id = state.next_cursor_id.wrapping_add(1);
    state.function_bodies.insert(body_key.clone(), Arc::new(node.body.clone()));

    // Extract the `def …:` text from current_source so the source of truth
    // for the body travels on the struct itself (no side channel).
    let source = extract_function_source(&state.current_source, node);

    // Capture closure (snapshot of current variables). `state.variables` is a
    // `HashMap` for lookup speed; `FunctionDef.closure` is a `BTreeMap` so
    // serialised snapshots stay deterministic.
    let closure: BTreeMap<String, Value> =
        state.variables.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

    let nonlocal_names = collect_nonlocal_names(&node.body);
    let nonlocal_cell_id = if nonlocal_names.is_empty() {
        None
    } else {
        let cell_id = state.next_nonlocal_cell_id;
        state.next_nonlocal_cell_id = state.next_nonlocal_cell_id.wrapping_add(1);
        // Seed the cell with current values of the nonlocal names
        // from the enclosing scope. After call_user_function writes
        // back, subsequent calls read these (mutated) values.
        let mut cell: rustc_hash::FxHashMap<String, Value> = rustc_hash::FxHashMap::default();
        for n in &nonlocal_names {
            if let Some(v) = state.variables.get(n) {
                cell.insert(n.clone(), v.clone());
            }
        }
        state.nonlocal_cells.insert(cell_id, cell);
        // Register the cell on the enclosing frame's owner map so
        // that further assignments to these names in the outer
        // function flow through to the cell (write-through). This
        // is what makes `outer reassigns n between inner-def and
        // inner-call` visible to inner on its next call.
        if let Some(owners) = state.frame_cell_owners.last_mut() {
            for n in &nonlocal_names {
                owners.insert(n.clone(), cell_id);
            }
        }
        Some(cell_id)
    };

    // Walk the body for `assigned_names` (what the checkpoint will
    // snapshot at call time) and `global_names` (which the checkpoint
    // must skip — those assignments persist to module scope).
    // `nonlocal`-declared names ride on the cell pattern and are
    // explicitly removed from `assigned_names` so they don't get
    // double-tracked.
    let (mut assigned_names, global_names) = collect_assigned_names(&node.body);
    assigned_names.retain(|n| !nonlocal_names.contains(n) && !global_names.contains(n));

    // call_depth==0 means this `def` happened at module scope, so
    // the closure entries are module globals (LEGB resolves reads to
    // the live module dict at call time, not a def-time snapshot).
    let is_module_level = state.call_depth == 0;

    let is_generator = contains_yield_stmts(&node.body);

    let mut func = Value::Function(std::sync::Arc::new(FunctionDef {
        name: name.to_string(),
        body_key,
        wraps_name: None,
        params,
        closure,
        source,
        nonlocal_names,
        is_generator,
        nonlocal_cell_id,
        assigned_names,
        global_names,
        is_module_level,
    }));

    // Apply decorators in REVERSE order so the textually nearest one
    // wraps first — the standard `@a\n@b\ndef f` is equivalent to
    // `f = a(b(f))`, with `b` applied before `a`.
    for decorator in node.decorator_list.iter().rev() {
        let dec_val = eval_expr(state, decorator, tools).await?;
        func = crate::eval::classes::apply_decorator(state, &dec_val, func, tools).await?;
    }

    state.set_variable(name, func).map_err(EvalError::Interpreter)?;
    Ok(Value::None)
}

/// Apply the latest nonlocal-cell values onto `state.variables`. Sync
/// helper kept outside `call_user_function`'s async future so the
/// HashMap clone doesn't bloat every recursive call's stack frame.
/// Error rollback is owned by the caller via the `VariableCheckpoint`
/// that already brackets the frame — this helper just propagates the
/// failure.
///
/// Cell-seed model: the cell is seeded with the enclosing scope's
/// value at inner-def time and updated by inner's writeback on each
/// call. Cell always wins over closure overlay (the cell is the
/// source of truth for nonlocal-bound names). Runtime assignments in
/// the owning outer frame write through via `frame_cell_owners` in
/// `InterpreterState::set_variable`, approximating CPython's shared
/// cell identity.
pub(super) fn apply_nonlocal_cell(
    state: &mut InterpreterState,
    func_def: &FunctionDef,
    local_scope: &rustc_hash::FxHashMap<String, Value>,
) -> Result<(), EvalError> {
    let Some(cell_id) = func_def.nonlocal_cell_id else { return Ok(()) };
    let Some(cell) = state.nonlocal_cells.get(&cell_id).cloned() else { return Ok(()) };
    for (name, value) in cell {
        if !local_scope.contains_key(&name) {
            state.set_variable(&name, value).map_err(EvalError::Interpreter)?;
        }
    }
    Ok(())
}

/// Apply a function-call frame's closure + local-scope overlay onto
/// `state.variables`. Extracted as a sync helper so the closure and
/// the per-step state don't survive across `call_user_function`'s
/// `execute_body(...).await` — every byte that lives across that await
/// inflates the recursive frame's native stack.
///
/// LEGB lookup is approximated here at frame entry:
///
/// - For a function defined at module scope (`is_module_level == true`), any closure entry that is
///   *currently* in `state.variables` is a module global. CPython resolves module-global reads to
///   the live module dict; overlay-from-snapshot would freeze the def-time value instead. So those
///   entries are skipped — the live value wins. Closure entries NOT in `state.variables` (e.g. the
///   function or others that have been deleted since) fall back to the def-time snapshot.
///
/// - For a nested def (`is_module_level == false`), the closure overlay is unconditional (except
///   for the `global` filter). This is load-bearing for decorator stacks: `@a @b def f` has both
///   `a`'s and `b`'s returned wrappers binding `fn` as a parameter; each inner frame MUST see its
///   own captured `fn`, not the surrounding frame's `fn`. Nonlocal cells use the
///   `frame_cell_owners` write-through model rather than first-class cell objects; add a ticket
///   before widening that model further.
///
/// `global`-declared names are deliberately skipped: their def-time
/// value would otherwise stomp on the live module value at every call.
/// The corresponding `VariableCheckpoint` also skips them so
/// subsequent assignments persist to the module scope.
pub(super) fn apply_function_scope(
    state: &mut InterpreterState,
    func_def: &FunctionDef,
    local_scope: &rustc_hash::FxHashMap<String, Value>,
) -> Result<(), EvalError> {
    for (name, value) in &func_def.closure {
        if local_scope.contains_key(name) || func_def.global_names.contains(name) {
            continue;
        }
        if func_def.is_module_level && state.variables.contains_key(name) {
            // Module-level def: live module dict wins.
            continue;
        }
        // Nested def: skip overlay when the live value byte-equals
        // the def-time closure value. That's the in-stack common
        // case where outer's local is the same logical object the
        // closure snapshotted; leaving it alone lets the body's
        // in-place mutations propagate to outer.
        if let Some(live) = state.variables.get(name) {
            if live == value {
                continue;
            }
        }
        state.set_variable(name, value.clone()).map_err(EvalError::Interpreter)?;
    }
    apply_nonlocal_cell(state, func_def, local_scope)?;
    for (name, value) in local_scope {
        state.set_variable(name, value.clone()).map_err(EvalError::Interpreter)?;
    }
    Ok(())
}

/// Same shape as [`apply_function_scope`] but for lambdas.
/// Module-level lambdas defer to the live module dict for free names
/// already present (LEGB read), matching the function-def rule.
pub(super) fn apply_lambda_scope(
    state: &mut InterpreterState,
    lambda_def: &LambdaDef,
    local_scope: &rustc_hash::FxHashMap<String, Value>,
) -> Result<(), EvalError> {
    for (name, value) in &lambda_def.closure {
        if local_scope.contains_key(name) {
            continue;
        }
        if lambda_def.is_module_level && state.variables.contains_key(name) {
            continue;
        }
        if let Some(live) = state.variables.get(name) {
            if live == value {
                continue;
            }
        }
        state.set_variable(name, value.clone()).map_err(EvalError::Interpreter)?;
    }
    for (name, value) in local_scope {
        state.set_variable(name, value.clone()).map_err(EvalError::Interpreter)?;
    }
    Ok(())
}

/// Copy the post-body values of `nonlocal`-declared names back to the
/// shared cell. Sync helper — see `apply_nonlocal_cell` for the
/// async-future-bloat reasoning.
pub(super) fn writeback_nonlocal_cell(state: &mut InterpreterState, func_def: &FunctionDef) {
    let Some(cell_id) = func_def.nonlocal_cell_id else { return };
    let writeback: Vec<(String, Value)> = func_def
        .nonlocal_names
        .iter()
        .filter_map(|n| state.variables.get(n).map(|v| (n.clone(), v.clone())))
        .collect();
    if let Some(cell) = state.nonlocal_cells.get_mut(&cell_id) {
        for (n, v) in writeback {
            cell.insert(n, v);
        }
    }
}

/// Scan a function body for `nonlocal x, y, ...` statements at the
/// top level. Names listed here trigger write-back to the enclosing
/// scope at call exit so `n += 1` inside the inner function persists
/// across calls. We only scan the function's own statements (not
/// statements inside nested functions); nested-function `nonlocal`
/// declarations bind against THAT function's enclosing scope, which
/// is handled when the nested function's own body is scanned.
fn collect_nonlocal_names(body: &[ast::Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    collect_nonlocal_names_inner(body, &mut names);
    names
}

fn collect_nonlocal_names_inner(body: &[ast::Stmt], out: &mut Vec<String>) {
    for stmt in body {
        match stmt {
            ast::Stmt::Nonlocal(node) => {
                for ident in &node.names {
                    let n = ident.as_str().to_string();
                    if !out.contains(&n) {
                        out.push(n);
                    }
                }
            }
            // Recurse into block-introducing statements (if/for/while/
            // with/try) so a `nonlocal` declared inside a conditional
            // branch still counts. Skip nested function/class bodies —
            // those have their own scope.
            ast::Stmt::If(node) => {
                collect_nonlocal_names_inner(&node.body, out);
                collect_nonlocal_names_inner(&node.orelse, out);
            }
            ast::Stmt::For(node) => {
                collect_nonlocal_names_inner(&node.body, out);
                collect_nonlocal_names_inner(&node.orelse, out);
            }
            ast::Stmt::While(node) => {
                collect_nonlocal_names_inner(&node.body, out);
                collect_nonlocal_names_inner(&node.orelse, out);
            }
            ast::Stmt::With(node) => {
                collect_nonlocal_names_inner(&node.body, out);
            }
            ast::Stmt::Try(node) => {
                collect_nonlocal_names_inner(&node.body, out);
                collect_nonlocal_names_inner(&node.orelse, out);
                collect_nonlocal_names_inner(&node.finalbody, out);
                for handler in &node.handlers {
                    let ast::ExceptHandler::ExceptHandler(h) = handler;
                    collect_nonlocal_names_inner(&h.body, out);
                }
            }
            _ => {}
        }
    }
}

/// Statically walk a function body to enumerate every name the frame
/// might rebind, plus every name declared `global`. Returned as
/// `(assigned, globals)`. Used by `VariableCheckpoint` at call time so
/// we snapshot only the names this frame can touch rather than
/// cloning the entire `state.variables` HashMap.
///
/// `assigned` includes: `=` / `+=` / `:=` targets, `for` loop vars,
/// `except as` and `with as` bindings, `import` / `from … import …` as
/// names, nested `def` / `class` names, and `del` targets. Recurses
/// into `if` / `for` / `while` / `with` / `try` blocks; skips nested
/// function and class bodies — those manage their own scope.
///
/// `globals` lists names declared `global x[, y, ...]`; assignments
/// to these persist to the module (enclosing) scope and MUST be
/// excluded from the checkpoint so the per-frame restore does not
/// wipe them out. Names appearing in both `nonlocal` (handled
/// separately by the cell pattern) and `global` should not appear in
/// `assigned` — the caller filters via the existing `nonlocal_names`
/// list.
pub(crate) fn collect_assigned_names(body: &[ast::Stmt]) -> (Vec<String>, Vec<String>) {
    let mut assigned = Vec::new();
    let mut globals = Vec::new();
    collect_assigned_names_inner(body, &mut assigned, &mut globals);
    (assigned, globals)
}

fn push_unique(out: &mut Vec<String>, name: &str) {
    let s = name.to_string();
    if !out.contains(&s) {
        out.push(s);
    }
}

/// Walk a target expression for `Assign` / `AugAssign` / `AnnAssign` /
/// `For` and extract every bound name. Handles `Tuple`/`List` unpacking
/// (`a, b = …`) and `Starred` (`*rest = …`). Attribute (`obj.x = y`)
/// and Subscript (`d[k] = v`) targets do NOT bind new names; skipped.
fn collect_target_names(target: &ast::Expr, out: &mut Vec<String>) {
    match target {
        ast::Expr::Name(n) => push_unique(out, n.id.as_str()),
        ast::Expr::Tuple(t) => {
            for elt in &t.elts {
                collect_target_names(elt, out);
            }
        }
        ast::Expr::List(l) => {
            for elt in &l.elts {
                collect_target_names(elt, out);
            }
        }
        ast::Expr::Starred(s) => collect_target_names(&s.value, out),
        // Attribute / Subscript / anything else doesn't introduce a
        // new binding at this scope — skip.
        _ => {}
    }
}

/// Recursively scan an expression for walrus targets (`name := value`).
/// PEP 572 binds walrus targets to the *enclosing function* scope, so a
/// walrus that appears anywhere inside this body — including nested
/// comprehensions — counts as a name assigned by this frame and must
/// land in `assigned_names` for the checkpoint to clean up on exit.
/// Nested `def` / `class` / `lambda` bodies are skipped (their walrus
/// targets bind to THEIR scope, not this one).
fn collect_walrus_targets(expr: &ast::Expr, out: &mut Vec<String>) {
    match expr {
        ast::Expr::NamedExpr(node) => {
            collect_target_names(&node.target, out);
            collect_walrus_targets(&node.value, out);
        }
        ast::Expr::BoolOp(node) => {
            for v in &node.values {
                collect_walrus_targets(v, out);
            }
        }
        ast::Expr::BinOp(node) => {
            collect_walrus_targets(&node.left, out);
            collect_walrus_targets(&node.right, out);
        }
        ast::Expr::UnaryOp(node) => collect_walrus_targets(&node.operand, out),
        ast::Expr::IfExp(node) => {
            collect_walrus_targets(&node.test, out);
            collect_walrus_targets(&node.body, out);
            collect_walrus_targets(&node.orelse, out);
        }
        ast::Expr::Compare(node) => {
            collect_walrus_targets(&node.left, out);
            for c in &node.comparators {
                collect_walrus_targets(c, out);
            }
        }
        ast::Expr::Call(node) => {
            collect_walrus_targets(&node.func, out);
            for a in &node.args {
                collect_walrus_targets(a, out);
            }
            for kw in &node.keywords {
                collect_walrus_targets(&kw.value, out);
            }
        }
        ast::Expr::Attribute(node) => collect_walrus_targets(&node.value, out),
        ast::Expr::Subscript(node) => {
            collect_walrus_targets(&node.value, out);
            collect_walrus_targets(&node.slice, out);
        }
        ast::Expr::Starred(node) => collect_walrus_targets(&node.value, out),
        ast::Expr::Tuple(node) => {
            for e in &node.elts {
                collect_walrus_targets(e, out);
            }
        }
        ast::Expr::List(node) => {
            for e in &node.elts {
                collect_walrus_targets(e, out);
            }
        }
        ast::Expr::Set(node) => {
            for e in &node.elts {
                collect_walrus_targets(e, out);
            }
        }
        ast::Expr::Dict(node) => {
            for k in node.keys.iter().flatten() {
                collect_walrus_targets(k, out);
            }
            for v in &node.values {
                collect_walrus_targets(v, out);
            }
        }
        ast::Expr::FormattedValue(node) => {
            collect_walrus_targets(&node.value, out);
            if let Some(fmt) = &node.format_spec {
                collect_walrus_targets(fmt, out);
            }
        }
        ast::Expr::JoinedStr(node) => {
            for v in &node.values {
                collect_walrus_targets(v, out);
            }
        }
        ast::Expr::Slice(node) => {
            if let Some(l) = &node.lower {
                collect_walrus_targets(l, out);
            }
            if let Some(u) = &node.upper {
                collect_walrus_targets(u, out);
            }
            if let Some(s) = &node.step {
                collect_walrus_targets(s, out);
            }
        }
        ast::Expr::Yield(node) => {
            if let Some(v) = &node.value {
                collect_walrus_targets(v, out);
            }
        }
        ast::Expr::YieldFrom(node) => collect_walrus_targets(&node.value, out),
        ast::Expr::Await(node) => collect_walrus_targets(&node.value, out),
        // PEP 572: a walrus inside a comprehension binds to the
        // COMPREHENSION's enclosing scope — which is exactly this
        // frame. Recurse into the element and the generators' iter
        // and condition expressions. Skip the generator target
        // (`for x in ...`) — that's local to the comprehension.
        ast::Expr::ListComp(node) => {
            collect_walrus_targets(&node.elt, out);
            for g in &node.generators {
                collect_walrus_targets(&g.iter, out);
                for c in &g.ifs {
                    collect_walrus_targets(c, out);
                }
            }
        }
        ast::Expr::SetComp(node) => {
            collect_walrus_targets(&node.elt, out);
            for g in &node.generators {
                collect_walrus_targets(&g.iter, out);
                for c in &g.ifs {
                    collect_walrus_targets(c, out);
                }
            }
        }
        ast::Expr::DictComp(node) => {
            collect_walrus_targets(&node.key, out);
            collect_walrus_targets(&node.value, out);
            for g in &node.generators {
                collect_walrus_targets(&g.iter, out);
                for c in &g.ifs {
                    collect_walrus_targets(c, out);
                }
            }
        }
        ast::Expr::GeneratorExp(node) => {
            collect_walrus_targets(&node.elt, out);
            for g in &node.generators {
                collect_walrus_targets(&g.iter, out);
                for c in &g.ifs {
                    collect_walrus_targets(c, out);
                }
            }
        }
        // Lambda has its own scope; leaf nodes (Name, Constant)
        // can't contain a walrus. Both fall through to no-op.
        _ => {}
    }
}

fn collect_assigned_names_inner(
    body: &[ast::Stmt],
    assigned: &mut Vec<String>,
    globals: &mut Vec<String>,
) {
    for stmt in body {
        match stmt {
            ast::Stmt::Global(node) => {
                for ident in &node.names {
                    push_unique(globals, ident.as_str());
                }
            }
            ast::Stmt::Assign(node) => {
                for target in &node.targets {
                    collect_target_names(target, assigned);
                }
                collect_walrus_targets(&node.value, assigned);
            }
            ast::Stmt::AugAssign(node) => {
                collect_target_names(&node.target, assigned);
                collect_walrus_targets(&node.value, assigned);
            }
            ast::Stmt::AnnAssign(node) => {
                collect_target_names(&node.target, assigned);
                if let Some(v) = &node.value {
                    collect_walrus_targets(v, assigned);
                }
            }
            ast::Stmt::Delete(node) => {
                for target in &node.targets {
                    collect_target_names(target, assigned);
                }
            }
            ast::Stmt::Expr(node) => collect_walrus_targets(&node.value, assigned),
            ast::Stmt::Return(node) => {
                if let Some(v) = &node.value {
                    collect_walrus_targets(v, assigned);
                }
            }
            ast::Stmt::Raise(node) => {
                if let Some(exc) = &node.exc {
                    collect_walrus_targets(exc, assigned);
                }
                if let Some(cause) = &node.cause {
                    collect_walrus_targets(cause, assigned);
                }
            }
            ast::Stmt::Assert(node) => {
                collect_walrus_targets(&node.test, assigned);
                if let Some(msg) = &node.msg {
                    collect_walrus_targets(msg, assigned);
                }
            }
            ast::Stmt::For(node) => {
                collect_target_names(&node.target, assigned);
                collect_walrus_targets(&node.iter, assigned);
                collect_assigned_names_inner(&node.body, assigned, globals);
                collect_assigned_names_inner(&node.orelse, assigned, globals);
            }
            ast::Stmt::AsyncFor(node) => {
                collect_target_names(&node.target, assigned);
                collect_walrus_targets(&node.iter, assigned);
                collect_assigned_names_inner(&node.body, assigned, globals);
                collect_assigned_names_inner(&node.orelse, assigned, globals);
            }
            ast::Stmt::While(node) => {
                collect_walrus_targets(&node.test, assigned);
                collect_assigned_names_inner(&node.body, assigned, globals);
                collect_assigned_names_inner(&node.orelse, assigned, globals);
            }
            ast::Stmt::If(node) => {
                collect_walrus_targets(&node.test, assigned);
                collect_assigned_names_inner(&node.body, assigned, globals);
                collect_assigned_names_inner(&node.orelse, assigned, globals);
            }
            ast::Stmt::With(node) => {
                for item in &node.items {
                    collect_walrus_targets(&item.context_expr, assigned);
                    if let Some(target) = &item.optional_vars {
                        collect_target_names(target, assigned);
                    }
                }
                collect_assigned_names_inner(&node.body, assigned, globals);
            }
            ast::Stmt::AsyncWith(node) => {
                for item in &node.items {
                    collect_walrus_targets(&item.context_expr, assigned);
                    if let Some(target) = &item.optional_vars {
                        collect_target_names(target, assigned);
                    }
                }
                collect_assigned_names_inner(&node.body, assigned, globals);
            }
            ast::Stmt::Try(node) => {
                collect_assigned_names_inner(&node.body, assigned, globals);
                collect_assigned_names_inner(&node.orelse, assigned, globals);
                collect_assigned_names_inner(&node.finalbody, assigned, globals);
                for handler in &node.handlers {
                    let ast::ExceptHandler::ExceptHandler(h) = handler;
                    if let Some(name) = &h.name {
                        push_unique(assigned, name.as_str());
                    }
                    if let Some(t) = &h.type_ {
                        collect_walrus_targets(t, assigned);
                    }
                    collect_assigned_names_inner(&h.body, assigned, globals);
                }
            }
            // `import x` binds `x`; `import x.y.z` binds `x` (the
            // leading component, not the dotted tail). `import x as y`
            // binds `y`. CPython's `compile.c` treats these the same
            // way.
            ast::Stmt::Import(node) => {
                for alias in &node.names {
                    let name = alias.asname.as_ref().map_or_else(
                        || {
                            alias
                                .name
                                .as_str()
                                .split('.')
                                .next()
                                .unwrap_or(alias.name.as_str())
                                .to_string()
                        },
                        |asname| asname.as_str().to_string(),
                    );
                    push_unique(assigned, &name);
                }
            }
            // `from foo import x[, y as z]` binds `x` and `z`. The
            // `from foo import *` form would introduce unknown names;
            // eval rejects it at module handling time, and
            // `gap-unsupported-error-anchor-gate` tracks keeping that
            // rejection consistently anchored.
            ast::Stmt::ImportFrom(node) => {
                for alias in &node.names {
                    if alias.name.as_str() == "*" {
                        continue;
                    }
                    let name = alias.asname.as_ref().map_or_else(
                        || alias.name.as_str().to_string(),
                        |a| a.as_str().to_string(),
                    );
                    push_unique(assigned, &name);
                }
            }
            ast::Stmt::FunctionDef(node) => {
                push_unique(assigned, node.name.as_str());
                // DO NOT recurse — nested def has its own scope.
            }
            ast::Stmt::AsyncFunctionDef(node) => {
                push_unique(assigned, node.name.as_str());
            }
            ast::Stmt::ClassDef(node) => {
                push_unique(assigned, node.name.as_str());
                // Same: nested class has its own scope.
            }
            // Pass, Break, Continue, Return, Raise, Expr, Nonlocal,
            // Match, AsyncFunctionDef/AsyncFor/AsyncWith already
            // handled above, etc. — no new bindings at THIS scope.
            _ => {}
        }
    }
}

/// Per-frame variable checkpoint. Records the pre-call value of every
/// name this frame might modify, scoped tight enough to skip the full
/// `state.variables.clone()` that previously dominated per-call cost.
///
/// `snapshots` carries `(name, Option<Value>)` — `None` marks "this
/// name did not exist before the frame entered," which means
/// `restore` removes it rather than restoring a previous value.
///
/// Names declared `global` in the function body are explicitly NOT
/// captured here — their assignments persist to the module scope by
/// design. The caller (the frame entry path) is responsible for
/// filtering them out before passing the touched-names list.
pub(crate) struct VariableCheckpoint {
    snapshots: Vec<(String, Option<Value>)>,
}

impl VariableCheckpoint {
    pub(crate) fn capture<I, S>(state: &InterpreterState, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let snapshots: Vec<(String, Option<Value>)> = names
            .into_iter()
            .map(|n| {
                let name = n.as_ref();
                let prev = state.variables.get(name).cloned();
                (name.to_string(), prev)
            })
            .collect();
        Self { snapshots }
    }

    pub(crate) fn restore(self, state: &mut InterpreterState) {
        for (name, prev) in self.snapshots {
            match prev {
                Some(v) => {
                    state.variables.insert(name, v);
                }
                None => {
                    state.variables.remove(&name);
                }
            }
        }
    }
}

/// Extract function source code from the current source using AST range.
pub(crate) fn extract_function_source(source: &str, node: &ast::StmtFunctionDef) -> String {
    use rustpython_parser::text_size::TextRange;
    let range: TextRange = node.range;
    let start = range.start().to_usize();
    let end = range.end().to_usize();
    if start < source.len() && end <= source.len() && start < end {
        source[start..end].to_string()
    } else {
        // Fallback: reconstruct a minimal stub
        format!("def {}(): pass", node.name)
    }
}

/// Build `FunctionParams` from an AST Arguments node.
pub fn build_function_params(args: &ast::Arguments) -> Result<FunctionParams, EvalError> {
    let positional: Vec<Param> = args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .map(|awd| Param { name: awd.def.arg.as_str().to_string() })
        .collect();

    // Defaults: stored as serialized AST (one per default, aligned to the tail of positional args)
    let all_args_with_default: Vec<&ast::ArgWithDefault> =
        args.posonlyargs.iter().chain(args.args.iter()).collect();
    let mut defaults: Vec<String> = Vec::new();
    for awd in &all_args_with_default {
        if let Some(ref default_expr) = awd.default {
            defaults.push(unparse_expr(default_expr)?);
        }
    }

    let kwonlyargs: Vec<Param> = args
        .kwonlyargs
        .iter()
        .map(|awd| Param { name: awd.def.arg.as_str().to_string() })
        .collect();

    let mut kw_defaults: Vec<Option<String>> = Vec::with_capacity(args.kwonlyargs.len());
    for awd in &args.kwonlyargs {
        kw_defaults.push(match &awd.default {
            Some(d) => Some(unparse_expr(d)?),
            None => None,
        });
    }

    let vararg = args.vararg.as_ref().map(|a| a.arg.as_str().to_string());
    let kwarg = args.kwarg.as_ref().map(|a| a.arg.as_str().to_string());

    Ok(FunctionParams {
        args: positional,
        defaults,
        default_values: Vec::new(),
        vararg,
        kwonlyargs,
        kw_defaults,
        kw_default_values: Vec::new(),
        kwarg,
    })
}

/// Convert an expression AST node back to Python source code.
///
/// Used to capture default-value expressions on `FunctionParams` so they can
/// be re-parsed and re-evaluated at each call (matching CPython's "defaults
/// are evaluated fresh" semantics) without holding a reference to the
/// original AST that `rustpython_parser` won't let us serialise.
/// Unparse the `for ... in ... [if ...]` clauses of a comprehension.
fn unparse_comprehensions(gens: &[ast::Comprehension]) -> Result<String, EvalError> {
    let mut parts = Vec::with_capacity(gens.len());
    for g in gens {
        let mut clause = format!("for {} in {}", unparse_expr(&g.target)?, unparse_expr(&g.iter)?);
        for cond in &g.ifs {
            clause.push_str(&format!(" if {}", unparse_expr(cond)?));
        }
        parts.push(clause);
    }
    Ok(parts.join(" "))
}

/// Unparse an f-string format spec (itself a `JoinedStr`): literal text plus any
/// nested `{width}`/`{prec}` replacement fields, without the surrounding colon.
fn unparse_format_spec(spec: &ast::Expr) -> Result<String, EvalError> {
    let ast::Expr::JoinedStr(js) = spec else {
        return unparse_expr(spec);
    };
    let mut out = String::new();
    for value in &js.values {
        match value {
            ast::Expr::Constant(ast::ExprConstant { value: ast::Constant::Str(s), .. }) => {
                out.push_str(s);
            }
            ast::Expr::FormattedValue(fv) => {
                out.push('{');
                out.push_str(&unparse_expr(&fv.value)?);
                out.push('}');
            }
            _ => {}
        }
    }
    Ok(out)
}

fn unparse_expr(expr: &ast::Expr) -> Result<String, EvalError> {
    // Unparse each element of a slice, joined by `sep`.
    let join = |exprs: &[ast::Expr], sep: &str| -> Result<String, EvalError> {
        Ok(exprs.iter().map(unparse_expr).collect::<Result<Vec<_>, _>>()?.join(sep))
    };

    Ok(match expr {
        ast::Expr::Constant(c) => match &c.value {
            ast::Constant::None => "None".to_string(),
            ast::Constant::Bool(true) => "True".to_string(),
            ast::Constant::Bool(false) => "False".to_string(),
            ast::Constant::Int(i) => format!("{i}"),
            ast::Constant::Float(f) => {
                if f.fract() == 0.0 && f.is_finite() {
                    format!("{f:.1}")
                } else {
                    format!("{f}")
                }
            }
            ast::Constant::Str(s) => format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'")),
            ast::Constant::Bytes(b) => format!("b'{}'", String::from_utf8_lossy(b)),
            ast::Constant::Ellipsis => "...".to_string(),
            ast::Constant::Tuple(items) => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|c| {
                        unparse_expr(&ast::Expr::Constant(ast::ExprConstant {
                            range: rustpython_parser::text_size::TextRange::default(),
                            value: c.clone(),
                            kind: None,
                        }))
                    })
                    .collect::<Result<_, _>>()?;
                format!("({})", parts.join(", "))
            }
            ast::Constant::Complex { real, imag } => format!("complex({real}, {imag})"),
        },
        ast::Expr::Name(n) => n.id.to_string(),
        // f-string default (`def f(s=f"{x}")`): reconstruct the `f"..."` source.
        // Literal chunks escape their braces; `{expr!conv:spec}` chunks unparse
        // the embedded expression, conversion flag, and (nested) format spec.
        ast::Expr::JoinedStr(js) => {
            let mut out = String::from("f\"");
            for value in &js.values {
                match value {
                    ast::Expr::Constant(ast::ExprConstant {
                        value: ast::Constant::Str(s), ..
                    }) => {
                        out.push_str(&s.replace('{', "{{").replace('}', "}}").replace('"', "\\\""));
                    }
                    ast::Expr::FormattedValue(fv) => {
                        out.push('{');
                        out.push_str(&unparse_expr(&fv.value)?);
                        match fv.conversion {
                            ast::ConversionFlag::Str => out.push_str("!s"),
                            ast::ConversionFlag::Repr => out.push_str("!r"),
                            ast::ConversionFlag::Ascii => out.push_str("!a"),
                            ast::ConversionFlag::None => {}
                        }
                        if let Some(spec) = &fv.format_spec {
                            out.push(':');
                            out.push_str(&unparse_format_spec(spec)?);
                        }
                        out.push('}');
                    }
                    other => {
                        return Err(InterpreterError::TypeError(format!(
                            "unsupported f-string default component (see CONFORMANCE.md#unsupported-language-features): {:?}",
                            std::mem::discriminant(other)
                        ))
                        .into());
                    }
                }
            }
            out.push('"');
            out
        }
        ast::Expr::List(l) => format!("[{}]", join(&l.elts, ", ")?),
        // Comprehension defaults (`def f(x=[i for i in range(3)])`).
        ast::Expr::ListComp(c) => {
            format!("[{} {}]", unparse_expr(&c.elt)?, unparse_comprehensions(&c.generators)?)
        }
        ast::Expr::SetComp(c) => {
            format!("{{{} {}}}", unparse_expr(&c.elt)?, unparse_comprehensions(&c.generators)?)
        }
        ast::Expr::GeneratorExp(c) => {
            format!("({} {})", unparse_expr(&c.elt)?, unparse_comprehensions(&c.generators)?)
        }
        ast::Expr::DictComp(c) => format!(
            "{{{}: {} {}}}",
            unparse_expr(&c.key)?,
            unparse_expr(&c.value)?,
            unparse_comprehensions(&c.generators)?
        ),
        ast::Expr::Set(s) => {
            // `{}` is an empty dict, never an empty set; a set literal always has
            // at least one element, so this branch is only reached with elements.
            format!("{{{}}}", join(&s.elts, ", ")?)
        }
        ast::Expr::Tuple(t) => {
            let parts: Vec<String> = t.elts.iter().map(unparse_expr).collect::<Result<_, _>>()?;
            if parts.len() == 1 {
                format!("({},)", parts[0])
            } else {
                format!("({})", parts.join(", "))
            }
        }
        ast::Expr::Dict(d) => {
            let mut parts = Vec::with_capacity(d.keys.len());
            for (k, v) in d.keys.iter().zip(d.values.iter()) {
                parts.push(match k {
                    None => format!("**{}", unparse_expr(v)?),
                    Some(key) => format!("{}: {}", unparse_expr(key)?, unparse_expr(v)?),
                });
            }
            format!("{{{}}}", parts.join(", "))
        }
        ast::Expr::UnaryOp(u) => {
            let op = match u.op {
                ast::UnaryOp::USub => "-",
                ast::UnaryOp::UAdd => "+",
                ast::UnaryOp::Not => "not ",
                ast::UnaryOp::Invert => "~",
            };
            format!("{op}{}", unparse_expr(&u.operand)?)
        }
        ast::Expr::BinOp(b) => {
            let op = match b.op {
                ast::Operator::Add => "+",
                ast::Operator::Sub => "-",
                ast::Operator::Mult => "*",
                ast::Operator::Div => "/",
                ast::Operator::FloorDiv => "//",
                ast::Operator::Mod => "%",
                ast::Operator::Pow => "**",
                ast::Operator::LShift => "<<",
                ast::Operator::RShift => ">>",
                ast::Operator::BitOr => "|",
                ast::Operator::BitXor => "^",
                ast::Operator::BitAnd => "&",
                ast::Operator::MatMult => "@",
            };
            format!("({} {op} {})", unparse_expr(&b.left)?, unparse_expr(&b.right)?)
        }
        ast::Expr::BoolOp(b) => {
            let op = match b.op {
                ast::BoolOp::And => " and ",
                ast::BoolOp::Or => " or ",
            };
            format!("({})", join(&b.values, op)?)
        }
        ast::Expr::Compare(c) => {
            let mut out = format!("({}", unparse_expr(&c.left)?);
            for (op, comparator) in c.ops.iter().zip(c.comparators.iter()) {
                let op = match op {
                    ast::CmpOp::Eq => "==",
                    ast::CmpOp::NotEq => "!=",
                    ast::CmpOp::Lt => "<",
                    ast::CmpOp::LtE => "<=",
                    ast::CmpOp::Gt => ">",
                    ast::CmpOp::GtE => ">=",
                    ast::CmpOp::Is => "is",
                    ast::CmpOp::IsNot => "is not",
                    ast::CmpOp::In => "in",
                    ast::CmpOp::NotIn => "not in",
                };
                out.push_str(&format!(" {op} {}", unparse_expr(comparator)?));
            }
            out.push(')');
            out
        }
        ast::Expr::IfExp(f) => format!(
            "({} if {} else {})",
            unparse_expr(&f.body)?,
            unparse_expr(&f.test)?,
            unparse_expr(&f.orelse)?,
        ),
        ast::Expr::Attribute(a) => format!("{}.{}", unparse_expr(&a.value)?, a.attr),
        ast::Expr::Subscript(s) => {
            format!("{}[{}]", unparse_expr(&s.value)?, unparse_expr(&s.slice)?)
        }
        ast::Expr::Slice(s) => {
            let part = |o: &Option<Box<ast::Expr>>| -> Result<String, EvalError> {
                match o {
                    Some(e) => unparse_expr(e),
                    None => Ok(String::new()),
                }
            };
            match &s.step {
                Some(step) => {
                    format!("{}:{}:{}", part(&s.lower)?, part(&s.upper)?, unparse_expr(step)?)
                }
                None => format!("{}:{}", part(&s.lower)?, part(&s.upper)?),
            }
        }
        ast::Expr::Starred(s) => format!("*{}", unparse_expr(&s.value)?),
        ast::Expr::Lambda(l) => {
            let params = build_function_params(&l.args)?;
            let mut names = params.args.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
            if let Some(v) = &params.vararg {
                names.push(format!("*{v}"));
            }
            for kw in &params.kwonlyargs {
                names.push(kw.name.clone());
            }
            if let Some(kw) = &params.kwarg {
                names.push(format!("**{kw}"));
            }
            format!("lambda {}: {}", names.join(", "), unparse_expr(&l.body)?)
        }
        ast::Expr::Call(c) => {
            let func = unparse_expr(&c.func)?;
            let mut arg_strs: Vec<String> =
                c.args.iter().map(unparse_expr).collect::<Result<_, _>>()?;
            for kw in &c.keywords {
                match &kw.arg {
                    Some(name) => arg_strs.push(format!("{name}={}", unparse_expr(&kw.value)?)),
                    None => arg_strs.push(format!("**{}", unparse_expr(&kw.value)?)),
                }
            }
            format!("{func}({})", arg_strs.join(", "))
        }
        // A default expression we cannot round-trip through source (comprehension,
        // f-string, walrus, yield, await, ...). CPython evaluates defaults once at
        // def time; if we cannot represent one, fail loudly at def time rather than
        // silently substituting `None` — the old fallback emitted a source comment
        // that parsed to None, so `def f(x=<unsupported>)` gave a wrong answer.
        other => {
            return Err(InterpreterError::TypeError(format!(
                "unsupported default argument expression (see CONFORMANCE.md#unsupported-language-features): {:?}",
                std::mem::discriminant(other)
            ))
            .into());
        }
    })
}

// ---------------------------------------------------------------------------
// Lambda
// ---------------------------------------------------------------------------

/// Evaluate a lambda definition — return a `Value::Lambda` (no closure capture).
/// Returns a Result because `evaluate_param_defaults` can fail when a default
/// expression references a name that isn't yet bound (CPython errors on the
/// same case at def time).
pub async fn eval_lambda_def(
    state: &mut InterpreterState,
    node: &ast::ExprLambda,
    tools: &Tools,
) -> EvalResult {
    let mut params = build_lambda_params(&node.args)?;

    // CPython evaluates lambda defaults at def time — the canonical
    // `lambda x, i=i: x + i` loop-capture idiom depends on this.
    // See the matching comment in `eval_function_def` for full
    // motivation.
    evaluate_param_defaults(state, &mut params, tools).await?;

    // Generate a unique ID for this lambda and store its body AST
    let lambda_id = format!("__lambda_{}", state.lambda_bodies.len());
    state.lambda_bodies.insert(lambda_id.clone(), Arc::new((*node.body).clone()));

    // Capture the original `lambda ...: ...` source text from
    // `current_source` using the node's range. Mirrors how
    // FunctionDef.source is extracted, but the slice here is
    // typically just `lambda x: x + 1`. On state import the source
    // is re-parsed to repopulate `lambda_bodies` for cross-execute
    // persistence.
    let source = extract_lambda_source(&state.current_source, node);

    let closure: BTreeMap<String, Value> =
        state.variables.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

    // Lambda bodies are expressions; the only binding form inside is
    // the walrus operator (`(x := …)`). PEP 572 binds a walrus in a
    // lambda body to the lambda's local scope, so the checkpoint
    // must snapshot any walrus targets we find here.
    let mut assigned_names = Vec::new();
    collect_walrus_targets(&node.body, &mut assigned_names);

    let is_module_level = state.call_depth == 0;

    Ok(Value::Lambda(std::sync::Arc::new(LambdaDef {
        params,
        lambda_id,
        source,
        closure,
        assigned_names,
        is_module_level,
    })))
}

/// Extract the `lambda <params>: <body>` text from `source` using
/// the AST node's byte range. Falls back to a stub if the offsets
/// don't slice cleanly (e.g. if the source was injected via state
/// import and the offsets are stale — that path doesn't re-evaluate
/// lambda defs so the fallback is only defensive).
fn extract_lambda_source(source: &str, node: &ast::ExprLambda) -> String {
    use rustpython_parser::text_size::TextRange;
    let range: TextRange = node.range;
    let start = range.start().to_usize();
    let end = range.end().to_usize();
    if start < source.len() && end <= source.len() && start < end {
        source[start..end].to_string()
    } else {
        "lambda: None".to_string()
    }
}

fn build_lambda_params(args: &ast::Arguments) -> Result<FunctionParams, EvalError> {
    build_function_params(args)
}

/// Scan a statement list for any `yield` or `yield from` expression.
/// Used by `call_user_function` to decide whether to switch to
/// generator mode. Walks every statement and recurses into nested
/// blocks (if / for / while / try / with / match), but does NOT
/// recurse into nested function or class bodies — a yield inside an
/// inner `def` belongs to THAT inner generator, not the outer one.
pub(crate) fn contains_yield_stmts(stmts: &[ast::Stmt]) -> bool {
    stmts.iter().any(contains_yield_stmt)
}

fn contains_yield_stmt(stmt: &ast::Stmt) -> bool {
    use ast::Stmt;
    match stmt {
        Stmt::Expr(e) => contains_yield_expr(&e.value),
        Stmt::Assign(a) => {
            contains_yield_expr(&a.value) || a.targets.iter().any(contains_yield_expr)
        }
        Stmt::AugAssign(a) => contains_yield_expr(&a.value) || contains_yield_expr(&a.target),
        Stmt::AnnAssign(a) => a.value.as_deref().is_some_and(contains_yield_expr),
        Stmt::Return(r) => r.value.as_deref().is_some_and(contains_yield_expr),
        Stmt::If(node) => {
            contains_yield_expr(&node.test)
                || contains_yield_stmts(&node.body)
                || contains_yield_stmts(&node.orelse)
        }
        Stmt::For(node) => {
            contains_yield_expr(&node.iter)
                || contains_yield_stmts(&node.body)
                || contains_yield_stmts(&node.orelse)
        }
        Stmt::While(node) => {
            contains_yield_expr(&node.test)
                || contains_yield_stmts(&node.body)
                || contains_yield_stmts(&node.orelse)
        }
        Stmt::Try(node) => {
            contains_yield_stmts(&node.body)
                || contains_yield_stmts(&node.orelse)
                || contains_yield_stmts(&node.finalbody)
                || node.handlers.iter().any(|h| match h {
                    ast::ExceptHandler::ExceptHandler(eh) => contains_yield_stmts(&eh.body),
                })
        }
        Stmt::With(node) => contains_yield_stmts(&node.body),
        Stmt::Match(node) => node.cases.iter().any(|c| contains_yield_stmts(&c.body)),
        Stmt::Raise(node) => {
            node.exc.as_deref().is_some_and(contains_yield_expr)
                || node.cause.as_deref().is_some_and(contains_yield_expr)
        }
        // Nested function / class definitions have their own scope (yield
        // inside them belongs to THAT scope, not the enclosing function),
        // and leaf statements (pass / break / continue / import / global
        // / nonlocal) carry no expressions. Both fall through to false.
        _ => false,
    }
}

pub(super) fn contains_yield_expr(expr: &ast::Expr) -> bool {
    use ast::Expr;
    match expr {
        Expr::Yield(_) | Expr::YieldFrom(_) => true,
        Expr::BoolOp(node) => node.values.iter().any(contains_yield_expr),
        Expr::BinOp(node) => contains_yield_expr(&node.left) || contains_yield_expr(&node.right),
        Expr::UnaryOp(node) => contains_yield_expr(&node.operand),
        Expr::IfExp(node) => {
            contains_yield_expr(&node.test)
                || contains_yield_expr(&node.body)
                || contains_yield_expr(&node.orelse)
        }
        Expr::Compare(node) => {
            contains_yield_expr(&node.left) || node.comparators.iter().any(contains_yield_expr)
        }
        Expr::Call(node) => {
            contains_yield_expr(&node.func)
                || node.args.iter().any(contains_yield_expr)
                || node.keywords.iter().any(|kw| contains_yield_expr(&kw.value))
        }
        Expr::Attribute(node) => contains_yield_expr(&node.value),
        Expr::Subscript(node) => {
            contains_yield_expr(&node.value) || contains_yield_expr(&node.slice)
        }
        Expr::Starred(node) => contains_yield_expr(&node.value),
        Expr::Tuple(node) => node.elts.iter().any(contains_yield_expr),
        Expr::List(node) => node.elts.iter().any(contains_yield_expr),
        Expr::Set(node) => node.elts.iter().any(contains_yield_expr),
        Expr::Dict(node) => {
            node.values.iter().any(contains_yield_expr)
                || node.keys.iter().any(|k| k.as_ref().is_some_and(contains_yield_expr))
        }
        Expr::JoinedStr(node) => node.values.iter().any(contains_yield_expr),
        Expr::FormattedValue(node) => contains_yield_expr(&node.value),
        Expr::NamedExpr(node) => contains_yield_expr(&node.value),
        // Comprehensions and lambdas have their own scope — a yield
        // inside creates an inner generator, not part of the outer
        // body. Same as leaves (literals, names, constants).
        _ => false,
    }
}
