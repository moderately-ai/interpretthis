// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emulation of Python's `typing` module — no-op identity helpers.
//!
//! The interpreter ignores type annotations at evaluation time, so
//! `typing` aliases (`List[int]`, `Dict[str, int]`, etc.) need only
//! resolve enough to make annotated code parse and run. Every name
//! we expose returns a sentinel-shaped value that:
//!
//!   * Can be subscripted (`List[int]`) and the subscript returns the original alias unchanged — so
//!     chained subscripts compose.
//!   * Can appear in a function signature annotation (annotations are stripped by the parser and
//!     don't reach the evaluator at binding time).
//!
//! `cast(typ, value)` is a true no-op identity — returns the value.

use crate::{
    error::{EvalResult, InterpreterError},
    value::Value,
};

/// Module-level constants — `typing.Any`, `typing.Final`, etc. all
/// resolve to a `Type` sentinel that subscripting returns
/// unchanged. The subscript behaviour is handled in `eval_subscript`
/// via Value::Type-on-LHS: subscripts on a Type value return the
/// Type unchanged.
pub fn constant(name: &str) -> Option<Value> {
    match name {
        // Generic-alias names that user code writes as annotations.
        "Any" | "Optional" | "Union" | "List" | "Dict" | "Set" | "Tuple" | "FrozenSet"
        | "Iterable" | "Iterator" | "Generator" | "Callable" | "Mapping" | "MutableMapping"
        | "Sequence" | "MutableSequence" | "Collection" | "Container" | "Hashable" | "Sized"
        | "Type" | "Final" | "Literal" | "ClassVar" | "Annotated" | "NoReturn" | "Never"
        | "Self" | "TypeAlias" | "TypeGuard" | "Concatenate" | "NamedTuple" | "TypedDict"
        | "Protocol" | "Generic" => Some(Value::Type(format!("typing.{name}"))),
        _ => None,
    }
}

pub fn has_function(name: &str) -> bool {
    matches!(
        name,
        "cast"
            | "NewType"
            | "TYPE_CHECKING"
            | "get_type_hints"
            | "get_args"
            | "get_origin"
            // `TypeVar`/`ParamSpec` are *called* to build type parameters
            // (`T = TypeVar("T")`), so they must resolve as callables rather
            // than bare `Type` sentinels.
            | "TypeVar"
            | "ParamSpec"
    )
}

pub fn call(func: &str, args: &[Value]) -> EvalResult {
    match func {
        // `cast(type, value)` is a runtime no-op — returns value.
        "cast" => args.get(1).cloned().ok_or_else(|| {
            InterpreterError::TypeError("cast() requires 2 arguments".into()).into()
        }),
        // `NewType(name, base)` returns a callable that's effectively
        // identity. We model it as the base type — calling NewType's
        // result returns the input unchanged.
        "NewType" => Ok(Value::Type(format!(
            "typing.NewType:{}",
            args.first()
                .and_then(|v| match v {
                    Value::String(s) => Some(s.as_str().to_owned()),
                    _ => None,
                })
                .unwrap_or_else(|| "anon".to_string())
        ))),
        // get_args / get_origin: return None / the type itself as a
        // sensible degenerate. Real introspection would need the
        // generic-alias machinery we don't model.
        "get_args" => Ok(Value::Tuple(Vec::new())),
        "get_origin" => Ok(Value::None),
        "get_type_hints" => Ok(Value::Dict(crate::value::shared_dict(indexmap::IndexMap::new()))),
        // `TypeVar("T", ...)` / `ParamSpec("P")` — build a type-parameter
        // sentinel named after the first argument. It only needs to be a
        // subscriptable value usable in `Generic[T]` and annotations, both of
        // which the evaluator erases; the name is carried for a faithful repr.
        "TypeVar" | "ParamSpec" => {
            let param_name = args.first().and_then(Value::as_str).unwrap_or("anon");
            // CPython reprs an invariant type parameter as `~T` (covariant `+T`,
            // contravariant `-T`); the `~` prefix is what shows in `Generic[~T]`
            // and bare `print(T)`. Variance kwargs are erased here, so default `~`.
            Ok(Value::Type(format!("~{param_name}")))
        }
        // TYPE_CHECKING is a constant False (the runtime check). The
        // constant() path also resolves it; calling it as a function
        // shouldn't happen but we return False defensively.
        "TYPE_CHECKING" => Ok(Value::Bool(false)),
        _ => Err(InterpreterError::AttributeError(format!(
            "module 'typing' has no attribute '{func}'"
        ))
        .into()),
    }
}

/// `typing` module registration.
pub struct TypingModule;

#[async_trait::async_trait]
impl crate::eval::modules::Module for TypingModule {
    fn name(&self) -> &'static str {
        "typing"
    }
    fn constant(&self, name: &str) -> Option<Value> {
        constant(name)
    }
    fn has_function(&self, name: &str) -> bool {
        has_function(name)
    }
    async fn call(
        &self,
        _state: &mut crate::state::InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &indexmap::IndexMap<String, Value>,
        _tools: &crate::tools::Tools,
    ) -> EvalResult {
        call(func, args)
    }
}
