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

/// Grow the host stack when the remaining headroom drops below this, matching
/// the async function-call path (`dispatch::grow_stack`). Lets the sync numeric
/// tree-evaluator recurse up to `max_recursion_depth` regardless of the base
/// stack size instead of overflowing first.
const STACK_RED_ZONE: usize = 512 * 1024;
const STACK_GROW_SIZE: usize = 32 * 1024 * 1024;

/// 1-based line number that byte `offset` falls on inside `source`.
/// Returns 1 if `offset` is past the end (caller passed a degenerate
/// or default range), so a stamped message still names *some* line
/// rather than swallowing the diagnostic entirely.
pub(crate) fn line_of(source: &str, offset: usize) -> usize {
    if offset > source.len() {
        return 1;
    }
    // Count newlines over the byte slice, not `source[..offset]` — a byte offset
    // that lands inside a multi-byte character (e.g. an em-dash in a comment)
    // would panic on the `str` slice while a byte-slice index never does.
    source.as_bytes()[..offset].iter().filter(|b| **b == b'\n').count() + 1
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

/// Wrap a *compound* statement's evaluation (`if`/`for`/`while`/`with`/`try`/
/// `match`/class body — the ones that recurse into a nested block) with the
/// expression-depth guard and on-demand stack growth, so deeply nested blocks
/// raise a catchable RecursionError instead of overflowing the host stack.
/// Simple statements and loop *iterations* (which re-enter `eval_body`, not
/// this macro) pay nothing.
macro_rules! guarded_block {
    ($state:ident, $line:ident, $call:expr) => {
        grow_expr(async move {
            $state.enter_expr().map_err(EvalError::Interpreter)?;
            let r = $call.await.map_err(|e| stamp_line(e, $line));
            $state.exit_expr();
            r
        })
    };
}

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
        Stmt::If(node) => {
            guarded_block!(state, stmt_line, control_flow::eval_if(state, node, tools))
        }
        Stmt::For(node) => {
            guarded_block!(state, stmt_line, control_flow::eval_for(state, node, tools))
        }
        Stmt::While(node) => {
            guarded_block!(state, stmt_line, control_flow::eval_while(state, node, tools))
        }
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
        // `async def` shares the `def` machinery — `StmtAsyncFunctionDef` has the
        // same fields — with the resulting function marked async so calling it
        // yields a coroutine.
        Stmt::AsyncFunctionDef(node) => Box::pin(async move {
            let fdef = rustpython_parser::ast::StmtFunctionDef {
                name: node.name.clone(),
                args: node.args.clone(),
                body: node.body.clone(),
                decorator_list: node.decorator_list.clone(),
                returns: node.returns.clone(),
                type_comment: node.type_comment.clone(),
                type_params: node.type_params.clone(),
                range: node.range,
            };
            functions::eval_function_def_with(state, &fdef, true, tools)
                .await
                .map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Try(node) => {
            guarded_block!(state, stmt_line, exceptions::eval_try(state, node, tools))
        }
        Stmt::TryStar(node) => {
            guarded_block!(state, stmt_line, exceptions::eval_try_star(state, node, tools))
        }
        Stmt::Raise(node) => Box::pin(async move {
            exceptions::eval_raise(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Assert(node) => Box::pin(async move {
            exceptions::eval_assert(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Delete(node) => Box::pin(async move {
            delete::eval_delete(state, node, tools).await.map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::Match(node) => {
            guarded_block!(state, stmt_line, match_stmt::eval_match(state, node, tools))
        }
        Stmt::With(node) => {
            guarded_block!(state, stmt_line, control_flow::eval_with(state, node, tools))
        }
        Stmt::Import(node) => {
            let r = modules::eval_import(state, node).map_err(|e| stamp_line(e, stmt_line));
            Box::pin(async move { r })
        }
        Stmt::ImportFrom(node) => Box::pin(async move {
            modules::eval_import_from(state, node, tools)
                .await
                .map_err(|e| stamp_line(e, stmt_line))
        }),
        Stmt::ClassDef(node) => {
            guarded_block!(state, stmt_line, classes::eval_class_def(state, node, tools))
        }
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
        Expr::BinOp(_) | Expr::Compare(_) => try_eval_numeric_expr_sync(state, expr),
        _ => None,
    }
}

/// Evaluate a numeric-only expression tree without allocating boxed futures.
///
/// This deliberately fires only for trees made of numeric constants/current
/// numeric variables plus `BinOp`/non-membership `Compare`. Names that resolve
/// to user instances, lazy proxies, strings/containers, builtins, or anything
/// else return `None` so the async path can preserve dunder/proxy semantics.
fn try_eval_numeric_expr_sync(state: &mut InterpreterState, expr: &Expr) -> Option<EvalResult> {
    let outcome = eval_numeric_unmetered(state, expr, 0)?;
    let op_count = match &outcome {
        Ok((_, count)) | Err((_, count)) => *count,
    };
    for _ in 0..op_count {
        if let Err(e) = state.increment_ops() {
            return Some(Err(EvalError::Interpreter(e)));
        }
    }
    Some(match outcome {
        Ok((value, _)) => Ok(value),
        Err((err, _)) => Err(err),
    })
}

type NumericSync = Result<(Value, usize), (EvalError, usize)>;

fn eval_numeric_unmetered(
    state: &InterpreterState,
    expr: &Expr,
    depth: u32,
) -> Option<NumericSync> {
    // A deeply left/right-nested numeric expression (`1+1+…+1` with tens of
    // thousands of terms) recurses here once per operator. Two guards, mirroring
    // the function-call recursion (see dispatch::grow_stack): the depth counter
    // raises a catchable RecursionError at the configured limit, and
    // `stacker::maybe_grow` grows the host stack on demand so the recursion
    // reaches that limit cleanly instead of overflowing first (an uncatchable
    // SIGABRT) on a small base stack. Real numeric expressions are shallow, so
    // maybe_grow never actually grows on the hot path — it's a cheap check.
    if depth >= state.config.max_recursion_depth {
        return Some(Err((
            crate::error::InterpreterError::RecursionLimitExceeded {
                limit: state.config.max_recursion_depth,
            }
            .into(),
            1,
        )));
    }
    stacker::maybe_grow(STACK_RED_ZONE, STACK_GROW_SIZE, || match expr {
        Expr::Constant(node) => {
            let value = literals::eval_constant(&node.value);
            is_sync_numeric(&value).then_some(Ok((value, 1)))
        }
        Expr::Name(node) => {
            let value = state.get_variable(node.id.as_str())?.clone();
            is_sync_numeric(&value).then_some(Ok((value, 1)))
        }
        Expr::BinOp(node) => {
            let left = eval_numeric_unmetered(state, &node.left, depth + 1)?;
            let (left, left_count) = match left {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
            let right = eval_numeric_unmetered(state, &node.right, depth + 1)?;
            let (right, right_count) = match right {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
            let count = left_count + right_count + 1;
            Some(
                operations::apply_binop(
                    &left,
                    &right,
                    node.op,
                    state.decimal_prec,
                    state.config.max_int_bits,
                )
                .map(|value| (value, count))
                .map_err(|err| (err, count)),
            )
        }
        Expr::Compare(node) => eval_numeric_compare_unmetered(state, node, depth),
        _ => None,
    })
}

fn eval_numeric_compare_unmetered(
    state: &InterpreterState,
    node: &rustpython_parser::ast::ExprCompare,
    depth: u32,
) -> Option<NumericSync> {
    if node.ops.iter().any(|op| {
        matches!(op, rustpython_parser::ast::CmpOp::In | rustpython_parser::ast::CmpOp::NotIn)
    }) {
        return None;
    }
    let left = eval_numeric_unmetered(state, &node.left, depth + 1)?;
    let (mut left, mut count) = match left {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    for (op, comparator) in node.ops.iter().zip(node.comparators.iter()) {
        let right = eval_numeric_unmetered(state, comparator, depth + 1)?;
        let (right, right_count) = match right {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        count += right_count + 1;
        let result = match op {
            // Route to the single `is` implementation rather than a bespoke
            // discriminant+equality check here — otherwise the sync numeric path
            // and the async path answer `is` differently.
            rustpython_parser::ast::CmpOp::Is => Ok(operations::values_is(&left, &right)),
            rustpython_parser::ast::CmpOp::IsNot => Ok(!operations::values_is(&left, &right)),
            _ => operations::compare_builtin(state, *op, &left, &right),
        };
        match result {
            Ok(true) => left = right,
            Ok(false) => return Some(Ok((Value::Bool(false), count))),
            Err(err) => return Some(Err((err, count))),
        }
    }
    Some(Ok((Value::Bool(true), count)))
}

const fn is_sync_numeric(value: &Value) -> bool {
    matches!(value, Value::Bool(_) | Value::Int(_) | Value::BigInt(_) | Value::Float(_))
}

/// Box an expression future so each poll runs under `stacker::maybe_grow`,
/// letting a deep recursive expression chain grow the host stack on demand
/// instead of overflowing it. Only the recursion-prone, non-bracketed arms of
/// [`eval_expr`] use this; the sync fast path never allocates it.
fn grow_expr<'a>(fut: impl Future<Output = EvalResult> + Send + 'a) -> EvalFut<'a> {
    Box::pin(functions::dispatch::grow_stack(fut))
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
        // These arms nest recursively through `eval_expr` without introducing a
        // bracket (`a.b.c.d…`, `not not…`, `"a"+"a"+…`, `x if p else x if p …`),
        // so a pathological chain would overflow the host stack before the
        // per-node op counter bounds it. Grow the stack on demand (as the
        // function-call path does) so deep chains evaluate — or hit the op /
        // memory limit — instead of aborting the process. Bracketed nesting
        // (nested list/call/subscript) is bounded earlier by the parse-time
        // depth guard; flat nodes (Compare, BoolOp) don't recurse.
        Expr::Attribute(node) => grow_expr(names::eval_attribute(state, node, tools)),
        Expr::Subscript(node) => grow_expr(names::eval_subscript(state, node, tools)),
        Expr::BinOp(node) => grow_expr(operations::eval_binop(state, node, tools)),
        Expr::UnaryOp(node) => grow_expr(operations::eval_unaryop(state, node, tools)),
        Expr::Compare(node) => Box::pin(operations::eval_compare(state, node, tools)),
        Expr::BoolOp(node) => Box::pin(operations::eval_boolop(state, node, tools)),
        Expr::IfExp(node) => grow_expr(operations::eval_ifexp(state, node, tools)),
        Expr::Call(node) => grow_expr(functions::eval_call(state, node, tools)),
        Expr::Lambda(node) => Box::pin(functions::eval_lambda_def(state, node, tools)),
        Expr::JoinedStr(node) => Box::pin(strings::eval_joined_str(state, node, tools)),
        Expr::FormattedValue(node) => Box::pin(strings::eval_formatted_value(state, node, tools)),
        Expr::ListComp(node) => Box::pin(comprehensions::eval_list_comp(state, node, tools)),
        Expr::DictComp(node) => Box::pin(comprehensions::eval_dict_comp(state, node, tools)),
        Expr::SetComp(node) => Box::pin(comprehensions::eval_set_comp(state, node, tools)),
        Expr::GeneratorExp(node) => {
            Box::pin(comprehensions::eval_generator_exp(state, node, tools))
        }
        Expr::NamedExpr(node) => grow_expr(names::eval_named_expr(state, node, tools)),
        Expr::Starred(node) => eval_expr(state, &node.value, tools),
        Expr::Slice(node) => Box::pin(names::eval_slice(state, node, tools)),
        Expr::Yield(node) => Box::pin(async move {
            // Resume: deliver send(value) — or raise a thrown exception — as the
            // result of this yield expr.
            if let Some(&id) = state.active_generator_stack.last() {
                if let Some(frame) = state.generators.get_mut(&id) {
                    if frame.resume_at_yield {
                        frame.resume_at_yield = false;
                        if let Some(exc) = frame.pending_throw.take() {
                            return Err(EvalError::Exception(*exc));
                        }
                        return Ok(std::mem::replace(&mut frame.send_value, Value::None));
                    }
                }
            }
            let value = if let Some(ref v) = node.value {
                eval_expr(state, v, tools).await?
            } else {
                Value::None
            };
            // Nested/eager path still uses yield_stack.
            if let Some(buffer) = state.yield_stack.last_mut() {
                buffer.push(value);
                return Ok(Value::None);
            }
            if !state.active_generator_stack.is_empty() {
                return Err(EvalError::Signal(ControlFlow::Yield(Box::new(value))));
            }
            Err(InterpreterError::Runtime(
                "'yield' outside function: yield can only appear inside a generator function"
                    .into(),
            )
            .into())
        }),
        Expr::YieldFrom(node) => Box::pin(async move {
            // On resume, the delegated iterable was already drained on the
            // first pass; hand back the captured sub-generator return value
            // (CPython: the value of `yield from` is that return value)
            // without re-evaluating and re-draining the sub-expression.
            if let Some(&id) = state.active_generator_stack.last() {
                if let Some(frame) = state.generators.get_mut(&id) {
                    if frame.resume_at_yield {
                        frame.resume_at_yield = false;
                        if let Some(exc) = frame.pending_throw.take() {
                            return Err(EvalError::Exception(*exc));
                        }
                        return Ok(frame.yield_from_return.take().unwrap_or(Value::None));
                    }
                }
            }
            let source = eval_expr(state, &node.value, tools).await?;

            // Streaming delegation to a true sub-generator: keep it suspended
            // and pull lazily so its `finally`/`.close()`/`throw()` run at the
            // correct time. `next`/`send`/`throw`/`close` on THIS generator are
            // forwarded to the sub in `dispatch_suspended` while `delegating_to`
            // is set. (The eager yield_stack path below handles `list(gen)`.)
            if let Value::Generator { id: sub_id } = &source {
                let sub_id = *sub_id;
                if state.yield_stack.is_empty() {
                    // Pull the first item from the sub-generator.
                    let empty = indexmap::IndexMap::new();
                    let first = crate::eval::functions::dispatch_generator_method(
                        state,
                        &Value::Generator { id: sub_id },
                        "__next__",
                        &[],
                        &empty,
                        tools,
                    )
                    .await;
                    return match first {
                        Ok(v) => {
                            if let Some(&id) = state.active_generator_stack.last() {
                                if let Some(frame) = state.generators.get_mut(&id) {
                                    frame.delegating_to = Some(sub_id);
                                }
                            }
                            Err(EvalError::Signal(ControlFlow::Yield(Box::new(v))))
                        }
                        // Sub returned without yielding: yield-from evaluates to
                        // its return value immediately.
                        Err(EvalError::Exception(e)) if e.type_name == "StopIteration" => {
                            Ok(e.args.first().cloned().unwrap_or(Value::None))
                        }
                        Err(other) => Err(other),
                    };
                }
            }

            // A delegated generator carries a `return` value in its
            // StopIteration; any other iterable's yield-from value is None.
            let (items, return_value) = match &source {
                Value::Generator { id } => {
                    crate::eval::op::drain_generator_with_return(state, *id, tools).await?
                }
                _ => (crate::eval::op::iter(state, &source, tools).await?, Value::None),
            };
            if let Some(buffer) = state.yield_stack.last_mut() {
                buffer.extend(items);
                return Ok(return_value);
            }
            // Streaming yield-from: yield the first item now, drain the rest
            // through for_stack, and surface `return_value` on resume.
            if let Some(&id) = state.active_generator_stack.last() {
                if let Some(frame) = state.generators.get_mut(&id) {
                    if items.is_empty() {
                        // Nothing to yield: the sub-generator returned without
                        // yielding, so yield-from evaluates to its return value.
                        return Ok(return_value);
                    }
                    frame.yield_from_return = Some(return_value);
                    let mut rest = items;
                    let first = rest.remove(0);
                    // Reuse for_stack as a synthetic loop over remaining items.
                    if !rest.is_empty() {
                        frame.for_stack.push(crate::state::GeneratorForState {
                            items: std::sync::Arc::new(rest),
                            pos: 0,
                            body_index: 0,
                            target: String::new(), // empty => pure yield-from drain
                            lazy_source: None,
                            current_item: None,
                        });
                    }
                    return Err(EvalError::Signal(ControlFlow::Yield(Box::new(first))));
                }
            }
            Err(InterpreterError::Runtime(
                "'yield from' outside function: yield from can only appear inside a generator function".into(),
            )
            .into())
        }),
        // `await expr`: drive a coroutine to its result. Any already-resolved
        // awaitable (an `asyncio.sleep`/`gather` result, which this sandbox
        // computes eagerly) passes through unchanged. Awaiting a non-awaitable
        // matches CPython's TypeError.
        Expr::Await(node) => Box::pin(async move {
            let awaited = eval_expr(state, &node.value, tools).await?;
            match awaited {
                Value::Coroutine(coro) => functions::drive_coroutine(state, &coro, tools).await,
                // asyncio.sleep / gather results are computed eagerly here, so
                // they arrive as ordinary values — pass them through.
                other => Ok(other),
            }
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

#[cfg(test)]
mod tests {
    use super::line_of;

    #[test]
    fn line_of_never_panics_on_offset_inside_multibyte_char() {
        // A body's AST offsets can be applied to a different source (a
        // bootstrapped class, a generator body, ...) that contains multi-byte
        // characters. Slicing `&str` by such an offset panicked; the byte-slice
        // form must be infallible. The em-dash occupies 3 bytes.
        let src = "a\n— dashed comment\nb = 1";
        assert_eq!(line_of(src, 0), 1);
        assert_eq!(line_of(src, 2), 2);
        // Offsets 3 and 4 fall inside the em-dash bytes — must not panic.
        assert_eq!(line_of(src, 3), 2);
        assert_eq!(line_of(src, 4), 2);
        assert_eq!(line_of(src, src.len()), 3);
        assert_eq!(line_of(src, src.len() + 100), 1);
    }
}
