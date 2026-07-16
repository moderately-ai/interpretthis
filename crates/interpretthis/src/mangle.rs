// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! CPython private-name mangling.
//!
//! CPython applies a compile-time textual transform: any identifier of the form
//! `__name` (two or more leading underscores, not ending in two or more
//! underscores) that occurs inside a class body is rewritten to
//! `_ClassName__name`, where `ClassName` is the enclosing class's name with
//! leading underscores stripped. This is what lets a subclass define its own
//! `__x` without clobbering a parent's `__x`, and is observable through
//! `hasattr(obj, "_Class__x")`.
//!
//! We reproduce it as a single [`Fold`] pass over the parsed AST (mirroring
//! CPython, which mangles at compile time), rewriting `Name`/`Attribute`
//! identifiers plus method/class binding names and `global`/`nonlocal`
//! declarations. Only class *bodies* introduce a mangling context — a nested
//! class's bases and decorators mangle in the *enclosing* context, so those are
//! folded before the new context is pushed.

use std::convert::Infallible;

use rustpython_ast::{
    Expr, Identifier, Stmt, StmtClassDef,
    fold::{self, Fold},
};

/// The recursive fold descends once per AST nesting level, so a pathologically
/// deep expression (`a.b.c…`, `not not …`) would overflow the base stack before
/// eval's own recursion guard runs. Grow on demand exactly like the evaluator
/// (`eval/mod.rs`), so the mangle pass survives any AST the parser accepts and
/// leaves the clean `RecursionError` to eval. Depth is already bounded by the
/// source-size / bracket-nesting caps, so the growth is bounded too.
const MANGLE_RED_ZONE: usize = 512 * 1024;
const MANGLE_GROW_SIZE: usize = 32 * 1024 * 1024;

/// An identifier is a private name iff it has two or more leading underscores
/// and does not end in two or more underscores (which excludes dunders like
/// `__init__` and all-underscore names like `__`).
fn is_private_name(ident: &str) -> bool {
    ident.starts_with("__") && !ident.ends_with("__")
}

struct PrivateNameMangler {
    /// Enclosing class names (leading underscores already stripped), innermost
    /// last. Empty at module scope. An entry may be empty when a class name is
    /// all underscores — CPython performs no mangling in that class's body.
    class_stack: Vec<String>,
}

impl PrivateNameMangler {
    /// The mangled spelling of `ident` in the current class context, or `None`
    /// when no mangling applies (module scope, all-underscore class, or a name
    /// that is not private).
    fn mangle(&self, ident: &str) -> Option<String> {
        let prefix = self.class_stack.last()?;
        if prefix.is_empty() || !is_private_name(ident) {
            return None;
        }
        Some(format!("_{prefix}{ident}"))
    }

    /// Apply [`Self::mangle`] to an [`Identifier`] in place.
    fn mangle_ident(&self, ident: &mut Identifier) {
        if let Some(mangled) = self.mangle(ident.as_str()) {
            *ident = Identifier::new(mangled);
        }
    }

    /// Fold a class definition: its name, bases, keywords, decorators, and type
    /// params mangle in the enclosing context; only its body mangles under the
    /// class's own (stripped) name.
    fn fold_class_def<U>(&mut self, node: StmtClassDef<U>) -> Result<Stmt<U>, Infallible> {
        let StmtClassDef { range, mut name, bases, keywords, body, decorator_list, type_params } =
            node;
        // The body context uses the ORIGINAL (pre-mangle) class name, stripped
        // of leading underscores.
        let body_prefix = name.trim_start_matches('_').to_owned();
        self.mangle_ident(&mut name);
        let bases = bases.into_iter().map(|b| self.fold_expr(b)).collect::<Result<Vec<_>, _>>()?;
        let keywords =
            keywords.into_iter().map(|k| self.fold_keyword(k)).collect::<Result<Vec<_>, _>>()?;
        let decorator_list =
            decorator_list.into_iter().map(|d| self.fold_expr(d)).collect::<Result<Vec<_>, _>>()?;
        let type_params = type_params
            .into_iter()
            .map(|t| self.fold_type_param(t))
            .collect::<Result<Vec<_>, _>>()?;
        self.class_stack.push(body_prefix);
        let body = body.into_iter().map(|s| self.fold_stmt(s)).collect::<Result<Vec<_>, _>>()?;
        self.class_stack.pop();
        Ok(Stmt::ClassDef(StmtClassDef {
            range,
            name,
            bases,
            keywords,
            body,
            decorator_list,
            type_params,
        }))
    }
}

impl<U> Fold<U> for PrivateNameMangler {
    type TargetU = U;
    type Error = Infallible;
    type UserContext = ();

    fn will_map_user(&mut self, _user: &U) -> Self::UserContext {}

    fn map_user(&mut self, user: U, (): ()) -> Result<Self::TargetU, Self::Error> {
        Ok(user)
    }

    fn fold_expr(&mut self, node: Expr<U>) -> Result<Expr<U>, Infallible> {
        stacker::maybe_grow(MANGLE_RED_ZONE, MANGLE_GROW_SIZE, move || {
            let node = match node {
                Expr::Attribute(mut attr) => {
                    self.mangle_ident(&mut attr.attr);
                    Expr::Attribute(attr)
                }
                Expr::Name(mut name) => {
                    self.mangle_ident(&mut name.id);
                    Expr::Name(name)
                }
                other => other,
            };
            // Recurse into children (e.g. the `value` of an Attribute).
            fold::fold_expr(self, node)
        })
    }

    fn fold_stmt(&mut self, node: Stmt<U>) -> Result<Stmt<U>, Infallible> {
        stacker::maybe_grow(MANGLE_RED_ZONE, MANGLE_GROW_SIZE, move || self.fold_stmt_inner(node))
    }
}

impl PrivateNameMangler {
    fn fold_stmt_inner<U>(&mut self, node: Stmt<U>) -> Result<Stmt<U>, Infallible> {
        match node {
            Stmt::ClassDef(class_def) => self.fold_class_def(class_def),
            Stmt::FunctionDef(mut func) => {
                self.mangle_ident(&mut func.name);
                fold::fold_stmt(self, Stmt::FunctionDef(func))
            }
            Stmt::AsyncFunctionDef(mut func) => {
                self.mangle_ident(&mut func.name);
                fold::fold_stmt(self, Stmt::AsyncFunctionDef(func))
            }
            Stmt::Global(mut global) => {
                for name in &mut global.names {
                    self.mangle_ident(name);
                }
                Ok(Stmt::Global(global))
            }
            Stmt::Nonlocal(mut nonlocal) => {
                for name in &mut nonlocal.names {
                    self.mangle_ident(name);
                }
                Ok(Stmt::Nonlocal(nonlocal))
            }
            other => fold::fold_stmt(self, other),
        }
    }
}

/// Rewrite every private name (`__x`) inside a class body to its mangled
/// `_ClassName__x` form, matching CPython's compile-time transform.
#[must_use]
pub fn mangle_private_names(suite: Vec<Stmt>) -> Vec<Stmt> {
    let mut mangler = PrivateNameMangler { class_stack: Vec::new() };
    suite
        .into_iter()
        .map(|stmt| match mangler.fold_stmt(stmt) {
            Ok(folded) => folded,
            Err(never) => match never {},
        })
        .collect()
}
