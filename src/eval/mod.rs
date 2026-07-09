// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::future::Future;
use std::pin::Pin;

use rustpython_parser::ast::{Expr, Ranged, Stmt};

use crate::{
    error::{ControlFlow, EvalError, EvalResult, InterpreterError},
    state::InterpreterState,
    tools::Tools,
    value::Value,
};

/// Boxed eval future. Each statement/expression arm returns its own box so the
/// concrete future state machine stays small — poll stack cost scales with the
/// active arm, not the max of every arm (the previous single-match `Box::pin`
/// design).
type EvalFut<'a> = Pin<Box<dyn Future<Output = EvalResult> + Send + 'a>>;

/// 1-based line number that byte `offset` falls on inside `source`.
/// Returns 1 if `offset` is past the end (caller passed a degenerate
/// or default range), so a stamped message still names *some* line
/// rather than swallowing the diagnostic entirely.
pub(crate) fn line_of(source: &str, offset: usize) -> usize {
    if offset > source.len() {
        return 1;
    }
    source[..offset].bytes().filter(|b| *b == b'\n').count() + 1
}

/// Stamp ` (at line N)` onto an error's user-visible message when it
/// doesn't already carry a line marker. Agent loops persist this string
/// as `errorMessage`; without the line they can't self-correct beyond
/// the simplest one-statement scripts.
///
/// Control-flow signals pass through unchanged — they're never surfaced
/// to the user. `Syntax` already carries line/col from the parser.
/// `RecursionLimitExceeded` and `StateFormatSuperseded` describe state
/// the source line wouldn't clarify.
pub(crate) fn stamp_line(err: EvalError, line: usize) -> EvalError {
    let suffix = format!(" (at line {line})");
    let already_stamped = |s: &str| s.contains("at line ");
    let line_u32 = u32::try_from(line).unwrap_or(u32::MAX);
    match err {
        EvalError::Signal(_) => err,
        EvalError::Interpreter(inner) => EvalError::Interpreter(match inner {
            InterpreterError::Syntax(_)
            | InterpreterError::RecursionLimitExceeded { .. }
            | InterpreterError::StateFormatSuperseded { .. } => inner,
            InterpreterError::Security(m) if !already_stamped(&m) => {
                InterpreterError::Security(format!("{m}{suffix}"))
            }
            InterpreterError::Runtime(m) if !already_stamped(&m) => {
                InterpreterError::Runtime(format!("{m}{suffix}"))
            }
            InterpreterError::LimitExceeded(m) if !already_stamped(&m) => {
                InterpreterError::LimitExceeded(format!("{m}{suffix}"))
            }
            InterpreterError::NameError(m) if !already_stamped(&m) => {
                InterpreterError::NameError(format!("{m}{suffix}"))
            }
            InterpreterError::TypeError(m) if !already_stamped(&m) => {
                InterpreterError::TypeError(format!("{m}{suffix}"))
            }
            InterpreterError::ValueError(m) if !already_stamped(&m) => {
                InterpreterError::ValueError(format!("{m}{suffix}"))
            }
            InterpreterError::AttributeError(m) if !already_stamped(&m) => {
                InterpreterError::AttributeError(format!("{m}{suffix}"))
            }
            InterpreterError::AssertionError(m) if !already_stamped(&m) => {
                InterpreterError::AssertionError(format!("{m}{suffix}"))
            }
            InterpreterError::Tool { tool_name, message } if !already_stamped(&message) => {
                InterpreterError::Tool { tool_name, message: format!("{message}{suffix}") }
            }
            other => other,
        }),
        // Exception variants: stamp via a SIDE field, not by mutating
        // message. Display() for Value::Exception renders just the
        // message so `print(e)` / `str(e)` / `f'{e}'` inside the user
        // script stay clean. The Interpreter::execute boundary
        // appends `(at line N)` from stamped_line to the final
        // host-facing error message — that's where the agent loop
        // wants the suffix.
        EvalError::Exception(exc) if exc.stamped_line.is_none() => {
            let mut rebuilt = exc;
            rebuilt.stamped_line = Some(line_u32);
            EvalError::Exception(rebuilt)
        }
        EvalError::Exception(exc) => EvalError::Exception(exc),
    }
}

pub mod classes;
pub mod comprehensions;
pub mod control_flow;
pub mod delete;
pub mod exceptions;
pub mod functions;
pub mod literals;
pub mod match_stmt;
pub mod modules;
pub mod names;
pub mod op;
pub mod operations;
pub mod place;
pub mod render;
pub mod statements;
pub mod strings;

/// Evaluate a single statement.
///
/// Each match arm is a **separately** `Box::pin`'d future so the polled
/// state machine is only as large as the active arm (not the max of all
/// arms). That cuts native stack use per recursive Python call and is
/// what lets realistic recursion depths work on default OS stacks.
pub fn eval_stmt<'a>(
    state: &'a mut InterpreterState,
    stmt: &'a Stmt,
    tools: &'a Tools,
) -> EvalFut<'a> {
    if let Err(e) = state.increment_ops() {
        return Box::pin(async move { Err(EvalError::Interpreter(e)) });
    }

    // When we're executing inside a function/lambda body, the byte
    // offsets in the body's AST nodes point into the source that
    // *defined* the body — not into the current execute()'s source.
    let stmt_line = {
        let active_source =
            state.body_source_stack.last().map_or(state.current_source.as_str(), String::as_str);
        line_of(active_source, stmt.range().start().to_usize())
    };

    match stmt {
        // Bare-expression statements whose value is a Constant are no-ops.
        Stmt::Expr(node) if matches!(node.value.as_ref(), Expr::Constant(_)) => {
            Box::pin(async move { Ok(Value::None) })
        }
        Stmt::Expr(node) => Box::pin(async move {
            eval_expr(state, &node.value, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Assign(node) => Box::pin(async move {
            statements::eval_assign(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::AugAssign(node) => Box::pin(async move {
            statements::eval_aug_assign(state, node, tools)
                .await
                .map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::AnnAssign(node) => Box::pin(async move {
            statements::eval_ann_assign(state, node, tools)
                .await
                .map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::If(node) => Box::pin(async move {
            control_flow::eval_if(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::For(node) => Box::pin(async move {
            control_flow::eval_for(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::While(node) => Box::pin(async move {
            control_flow::eval_while(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Break(_) => Box::pin(async move { Err(EvalError::Signal(ControlFlow::Break)) }),
        Stmt::Continue(_) => Box::pin(async move { Err(EvalError::Signal(ControlFlow::Continue)) }),
        Stmt::Return(node) => Box::pin(async move {
            let val_result = if let Some(ref v) = node.value {
                eval_expr(state, v, tools).await
            } else {
                Ok(Value::None)
            };
            match val_result {
                Ok(val) => Err(stamp_line(
                    EvalError::Signal(ControlFlow::Return(Box::new(val))),
                    stmt_line,
                )),
                Err(e) => Err(stamp_line(e, stmt_line)),
            }
        }),
        Stmt::FunctionDef(node) => Box::pin(async move {
            functions::eval_function_def(state, node, tools)
                .await
                .map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Try(node) => Box::pin(async move {
            exceptions::eval_try(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::TryStar(node) => Box::pin(async move {
            exceptions::eval_try_star(state, node, tools)
                .await
                .map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Raise(node) => Box::pin(async move {
            exceptions::eval_raise(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Assert(node) => Box::pin(async move {
            exceptions::eval_assert(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Delete(node) => Box::pin(async move {
            delete::eval_delete(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Match(node) => Box::pin(async move {
            match_stmt::eval_match(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::With(node) => Box::pin(async move {
            control_flow::eval_with(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Import(node) => {
            let r = modules::eval_import(state, node).map_err(|e| stamp_line(e, stmt_line));
            Box::pin(async move { r })
        }
        Stmt::ImportFrom(node) => {
            let r = modules::eval_import_from(state, node).map_err(|e| stamp_line(e, stmt_line));
            Box::pin(async move { r })
        }
        Stmt::ClassDef(node) => Box::pin(async move {
            classes::eval_class_def(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Pass(_) | Stmt::Global(_) | Stmt::Nonlocal(_) => {
            Box::pin(async move { Ok(Value::None) })
        }
        _ => {
            let msg = format!(
                "unsupported statement: {:?} (see CONFORMANCE.md#unsupported-language-features)",
                std::mem::discriminant(stmt)
            );
            Box::pin(
                async move { Err(stamp_line(InterpreterError::Runtime(msg).into(), stmt_line)) },
            )
        }
    }
}

/// Try to evaluate an expression synchronously, without allocating a
/// `Box::pin`'d future. Returns `Some(result)` for leaf shapes whose
/// evaluation needs no async dispatch (Constant literals, plain Name
/// reads); returns `None` for every other shape, signalling the caller
/// to fall back to the async [`eval_expr`] path.
///
/// `Expr::Constant` and `Expr::Name` together cover the bulk of inner-
/// loop expression evaluations on realistic LLM-emitted snippets — every
/// `i + 1`, `d[5]`, `r["category"]` is a small tree whose leaves are one
/// or both of those. Skipping the `Box::pin` allocation per leaf saves
/// ~64 B of heap traffic per call and removes a future-state-machine
/// poll cycle that the optimiser otherwise can't elide.
///
/// Op-count increment still happens here — the budget cap doesn't get
/// to look the other way because the path is sync.
#[inline]
pub fn try_eval_expr_sync(
    state: &mut InterpreterState,
    expr: &Expr,
    tools: &Tools,
) -> Option<EvalResult> {
    match expr {
        Expr::Constant(node) => {
            if let Err(e) = state.increment_ops() {
                return Some(Err(EvalError::Interpreter(e)));
            }
            Some(Ok(literals::eval_constant(&node.value)))
        }
        Expr::Name(node) => {
            if let Err(e) = state.increment_ops() {
                return Some(Err(EvalError::Interpreter(e)));
            }
            Some(names::eval_name(state, node, tools))
        }
        _ => None,
    }
}

/// Evaluate a single expression.
///
/// Each match arm is a separately boxed future (see [`eval_stmt`]) so
/// recursive expression trees don't pay the max-arm state-machine size
/// on every poll.
///
/// Hot callers should try [`try_eval_expr_sync`] first to skip the boxed
/// future on Constant / Name leaves.
pub fn eval_expr<'a>(
    state: &'a mut InterpreterState,
    expr: &'a Expr,
    tools: &'a Tools,
) -> EvalFut<'a> {
    if let Some(fast) = try_eval_expr_sync(state, expr, tools) {
        return Box::pin(async move { fast });
    }

    if let Err(e) = state.increment_ops() {
        return Box::pin(async move { Err(EvalError::Interpreter(e)) });
    }

    match expr {
        Expr::Constant(node) => {
            let v = literals::eval_constant(&node.value);
            Box::pin(async move { Ok(v) })
        }
        Expr::List(node) => Box::pin(literals::eval_list(state, node, tools)),
        Expr::Tuple(node) => Box::pin(literals::eval_tuple(state, node, tools)),
        Expr::Dict(node) => Box::pin(literals::eval_dict(state, node, tools)),
        Expr::Set(node) => Box::pin(literals::eval_set(state, node, tools)),
        Expr::Name(node) => {
            let r = names::eval_name(state, node, tools);
            Box::pin(async move { r })
        }
        Expr::Attribute(node) => Box::pin(names::eval_attribute(state, node, tools)),
        Expr::Subscript(node) => Box::pin(names::eval_subscript(state, node, tools)),
        Expr::BinOp(node) => Box::pin(operations::eval_binop(state, node, tools)),
        Expr::UnaryOp(node) => Box::pin(operations::eval_unaryop(state, node, tools)),
        Expr::Compare(node) => Box::pin(operations::eval_compare(state, node, tools)),
        Expr::BoolOp(node) => Box::pin(operations::eval_boolop(state, node, tools)),
        Expr::IfExp(node) => Box::pin(operations::eval_ifexp(state, node, tools)),
        Expr::Call(node) => Box::pin(functions::eval_call(state, node, tools)),
        Expr::Lambda(node) => Box::pin(functions::eval_lambda_def(state, node, tools)),
        Expr::JoinedStr(node) => Box::pin(strings::eval_joined_str(state, node, tools)),
        Expr::FormattedValue(node) => Box::pin(strings::eval_formatted_value(state, node, tools)),
        Expr::ListComp(node) => Box::pin(comprehensions::eval_list_comp(state, node, tools)),
        Expr::DictComp(node) => Box::pin(comprehensions::eval_dict_comp(state, node, tools)),
        Expr::SetComp(node) => Box::pin(comprehensions::eval_set_comp(state, node, tools)),
        Expr::GeneratorExp(node) => {
            Box::pin(comprehensions::eval_generator_exp(state, node, tools))
        }
        Expr::NamedExpr(node) => Box::pin(names::eval_named_expr(state, node, tools)),
        Expr::Starred(node) => eval_expr(state, &node.value, tools),
        Expr::Slice(node) => Box::pin(names::eval_slice(state, node, tools)),
        Expr::Yield(node) => Box::pin(async move {
            let value = if let Some(ref v) = node.value {
                eval_expr(state, v, tools).await?
            } else {
                Value::None
            };
            let Some(buffer) = state.yield_stack.last_mut() else {
                return Err(InterpreterError::Runtime(
                    "'yield' outside function: yield can only appear inside a generator function"
                        .into(),
                )
                .into());
            };
            buffer.push(value);
            Ok(Value::None)
        }),
        Expr::YieldFrom(node) => Box::pin(async move {
            let source = eval_expr(state, &node.value, tools).await?;
            let items = crate::eval::op::iter(state, &source, tools).await?;
            let Some(buffer) = state.yield_stack.last_mut() else {
                return Err(InterpreterError::Runtime(
                    "'yield from' outside function: yield from can only appear inside a generator function".into(),
                )
                .into());
            };
            buffer.extend(items);
            Ok(Value::None)
        }),
        Expr::Await(_) => Box::pin(async move {
            Err(InterpreterError::Runtime(
                "'await' is not supported (see CONFORMANCE.md#unsupported-language-features)"
                    .into(),
            )
            .into())
        }),
    }
}

/// Evaluate a list of statements, returning the last value.
pub async fn eval_body(state: &mut InterpreterState, body: &[Stmt], tools: &Tools) -> EvalResult {
    let mut result = Value::None;
    for stmt in body {
        result = eval_stmt(state, stmt, tools).await?;
    }
    Ok(result)
}
