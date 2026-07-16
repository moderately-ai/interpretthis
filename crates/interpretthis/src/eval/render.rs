// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! State-aware `Value` formatting — the single canonical path.
//!
//! `print()`, the `repr()` builtin, and the f-string `!s` / `!r` / `!a`
//! conversion arms all route through [`render`]. The two stateless
//! paths (`impl fmt::Display for Value`, `Value::repr()`) stay as
//! best-effort fallbacks for sites without `&InterpreterState`
//! access (debug output, error chains); they render `Value::Instance`
//! as `<ClassName object>` because they cannot consult the class
//! registry for a `@dataclass`-synthesized `__repr__` or for a
//! user-defined `__str__` / `__repr__` slot.
//!
//! User-class `__str__` / `__repr__` dispatch lives here too: when an
//! Instance arrives at [`render`] and its class defines the relevant
//! slot, the slot runs via [`crate::eval::classes::call_method`] and
//! its return value is rendered as the final string. That requires
//! `&mut InterpreterState` + a `&Tools` reference, which is why
//! `render` is async and returns `EvalResult<String>`.

use indexmap::IndexMap;

use crate::{
    error::{EvalError, InterpreterError},
    eval::{
        classes::{call_method, lookup_method_in_mro},
        functions::CallArgs,
    },
    state::InterpreterState,
    tools::Tools,
    value::{DataclassField, Value},
};

/// How a `Value` is being formatted. Mirrors CPython's three conversion
/// flags: `str()`/`!s` for [`Display`], `repr()`/`!r` for [`Repr`],
/// `ascii()`/`!a` for [`Ascii`] (Repr-shape with non-ASCII chars escaped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Display,
    Repr,
    Ascii,
}

/// Render `value` to its CPython-equivalent string form, dispatching
/// `__str__` / `__repr__` on user-class instances when defined, and
/// falling back to the `@dataclass` synthesized `__repr__` when the
/// class is a dataclass without an explicit slot.
///
/// Async because slot dispatch runs a Python method body via
/// `call_method`, which is itself async. Stateless callers (Display
/// impl, `Value::repr`) keep the synchronous fallback shape — they
/// can't reach the class registry from inside a `fmt::Formatter`.
pub fn render<'a>(
    state: &'a mut InterpreterState,
    value: &'a Value,
    mode: RenderMode,
    tools: &'a Tools,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, EvalError>> + Send + 'a>> {
    Box::pin(async move {
        if let Value::Instance(inst) = value {
            // User-defined slot wins over the synthesized dataclass
            // repr and over the default `<ClassName object>`. Mode
            // selects which slot — `__str__` for Display/Ascii (since
            // CPython falls back to `__str__` when `__repr__` is
            // missing for str(), but Ascii is repr-derived… see arm
            // ordering below).
            let slot_name = match mode {
                RenderMode::Display => "__str__",
                RenderMode::Repr | RenderMode::Ascii => "__repr__",
            };
            if let Some((_, method)) = lookup_method_in_mro(state, &inst.class_name, slot_name) {
                let call = CallArgs { positional: &[], keyword: &IndexMap::new() };
                let (returned, _self_after) =
                    call_method(state, &method, value.clone(), call, tools).await?;
                let rendered = match &returned {
                    Value::String(s) => s.to_string(),
                    other => {
                        return Err(EvalError::from(InterpreterError::TypeError(format!(
                            "{}.{} returned non-string (type {})",
                            inst.class_name,
                            slot_name,
                            other.type_name()
                        ))));
                    }
                };
                return Ok(match mode {
                    RenderMode::Ascii => ascii_escape(&rendered),
                    _ => rendered,
                });
            }
            // CPython: str() falls back to repr() when __str__ is
            // absent but __repr__ is present. Honour that fallback
            // for user classes too.
            if matches!(mode, RenderMode::Display) {
                if let Some((_, method)) = lookup_method_in_mro(state, &inst.class_name, "__repr__")
                {
                    let call = CallArgs { positional: &[], keyword: &IndexMap::new() };
                    let (returned, _self_after) =
                        call_method(state, &method, value.clone(), call, tools).await?;
                    if let Value::String(s) = returned {
                        return Ok(s.into());
                    }
                }
            }
            // Dataclass synthesis — only when no explicit slot.
            if let Some(class) = state.classes.get(&inst.class_name) {
                if let Some(fields) = class.dataclass_fields.clone() {
                    return Ok(render_dataclass(state, &inst.class_name, inst, &fields));
                }
                // A `collections.namedtuple` reprs as `Name(field=value, …)`.
                if let Some(Value::Tuple(field_names)) = class.class_attrs.get("_fields") {
                    let names: Vec<String> = field_names
                        .iter()
                        .filter_map(|n| match n {
                            Value::String(s) => Some(s.to_string()),
                            _ => None,
                        })
                        .collect();
                    let mut out = format!("{}(", inst.class_name);
                    for (i, name) in names.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        let field_val =
                            inst.fields.lock().get(name.as_str()).cloned().unwrap_or(Value::None);
                        out.push_str(name);
                        out.push('=');
                        out.push_str(&render(state, &field_val, RenderMode::Repr, tools).await?);
                    }
                    out.push(')');
                    return Ok(out);
                }
            }
            return Ok(format!("<{} object>", inst.class_name));
        }
        // Containers recurse so an Instance inside a list/tuple/dict/
        // set picks up its __str__/__repr__ via render. CPython
        // formats list/tuple/dict/set elements via repr() regardless
        // of the outer mode, so we always render children as Repr.
        match value {
            Value::List(items) => {
                // A container already being rendered higher on the stack is a
                // cycle — emit `[...]` (CPython's `Py_ReprEnter` guard). The
                // active set lives in `state` (not a thread-local): `render` is
                // async and its task can migrate worker threads across an await
                // under the node / async-Python runtimes, which would corrupt a
                // thread-local set.
                let ptr = std::sync::Arc::as_ptr(items) as usize;
                if !state.repr_active.insert(ptr) {
                    return Ok("[...]".to_string());
                }
                // Snapshot the items under the lock — render_sequence
                // is async and walks recursively, so we can't hold a
                // parking_lot guard across awaits.
                let snapshot = items.lock().clone();
                let result = render_sequence(state, &snapshot, "[", "]", tools).await;
                state.repr_active.remove(&ptr);
                result
            }
            Value::Tuple(items) => {
                // `render_sequence` here uses an empty closing delimiter, so the
                // one-element tuple just needs a trailing `,)` — never trim the
                // body, or an element whose repr ends in `)` (e.g. `Decimal('1')`)
                // loses its own paren.
                let single = items.len() == 1;
                let body = render_sequence(state, items, "(", "", tools).await?;
                Ok(if single { format!("{body},)") } else { body + ")" })
            }
            Value::Set(body) => {
                let seq = body.lock().iter_ordered();
                if seq.is_empty() {
                    Ok("set()".to_string())
                } else {
                    Ok(render_sequence(state, &seq, "{", "}", tools).await?)
                }
            }
            Value::Frozenset(body) => {
                let seq = body.iter_ordered();
                if seq.is_empty() {
                    Ok("frozenset()".to_string())
                } else {
                    let inner = render_sequence(state, &seq, "{", "}", tools).await?;
                    Ok(format!("frozenset({inner})"))
                }
            }
            Value::Dict(map) => {
                let ptr = std::sync::Arc::as_ptr(map) as usize;
                if !state.repr_active.insert(ptr) {
                    return Ok("{...}".to_string());
                }
                let snapshot = map.lock().clone();
                // Await first, then remove the pointer — so an error mid-render
                // (e.g. a value's `__repr__` raising) still clears the guard and
                // a later repr of the same dict isn't wrongly `{...}`.
                let result = render_mapping(state, &snapshot, tools).await;
                state.repr_active.remove(&ptr);
                result
            }
            // A user exception subclass may override `__str__` / `__repr__`
            // (`def __str__(self): return f"[{self.code}] {self.args[0]}"`).
            // Dispatch it with the exception as `self` — attribute access
            // (`self.args`, `self.code`) resolves through the exception's fields
            // — so str/repr/f-string honour the override before the default
            // BaseException rendering below.
            Value::Exception(e)
                if lookup_method_in_mro(
                    state,
                    &e.type_name,
                    if matches!(mode, RenderMode::Display) { "__str__" } else { "__repr__" },
                )
                .is_some() =>
            {
                let slot = if matches!(mode, RenderMode::Display) { "__str__" } else { "__repr__" };
                let (_, method) =
                    lookup_method_in_mro(state, &e.type_name, slot).ok_or_else(|| {
                        EvalError::from(InterpreterError::Runtime(
                            "exception render slot vanished after guard".into(),
                        ))
                    })?;
                let call = CallArgs { positional: &[], keyword: &IndexMap::new() };
                let (returned, _self) =
                    call_method(state, &method, value.clone(), call, tools).await?;
                match returned {
                    Value::String(s) => Ok(match mode {
                        RenderMode::Ascii => ascii_escape(&s),
                        _ => s.into(),
                    }),
                    other => Err(EvalError::from(InterpreterError::TypeError(format!(
                        "{}.{slot} returned non-string (type {})",
                        e.type_name,
                        other.type_name()
                    )))),
                }
            }
            // `str(exc)` mirrors CPython's BaseException.__str__: no args
            // renders empty, a single arg renders as that arg's str, and
            // multiple args render as the args tuple's repr. `repr(exc)`
            // keeps the `Type(args…)` form from `value.repr()`.
            //
            // KeyError overrides __str__ to render its single arg via repr
            // (`str(KeyError('k'))` is "'k'"), so it takes the tuple-repr path
            // even for one arg.
            Value::Exception(e) if matches!(mode, RenderMode::Display) => match e.args.as_slice() {
                [] => Ok(String::new()),
                // KeyError renders its key via repr (`str(KeyError('k'))` is
                // "'k'"). The message is already the repr'd form at every
                // construction site (internal raisers quote the key; the
                // user-facing constructor reprs it — see construct_exception_type),
                // so return it directly rather than repr-ing the arg twice.
                [_] if e.type_name == "KeyError" => Ok(e.message.clone()),
                [single] => render(state, single, RenderMode::Display, tools).await,
                many => Ok(render_sequence(state, many, "(", "", tools).await? + ")"),
            },
            // A user class reprs as `<class '__main__.Qualname'>`. The
            // state-aware path supplies the dotted qualname for nested classes
            // (`Outer.Inner`); the stateless `Display` fallback only has the
            // bare name.
            Value::Class(name) => {
                let qualname = state
                    .classes
                    .get(name)
                    .map(|c| if c.qualname.is_empty() { &c.name } else { &c.qualname })
                    .unwrap_or(name);
                Ok(format!("<class '__main__.{qualname}'>"))
            }
            _ => Ok(match mode {
                RenderMode::Repr => value.repr(),
                RenderMode::Display => format!("{value}"),
                RenderMode::Ascii => ascii_escape(&value.repr()),
            }),
        }
    })
}

async fn render_sequence(
    state: &mut InterpreterState,
    items: &[Value],
    open: &str,
    close: &str,
    tools: &Tools,
) -> Result<String, EvalError> {
    let mut out = String::from(open);
    let mut first = true;
    for item in items {
        if !first {
            out.push_str(", ");
        }
        first = false;
        out.push_str(&render(state, item, RenderMode::Repr, tools).await?);
    }
    out.push_str(close);
    Ok(out)
}

/// Render a snapshot of dict entries as `{k: v, ...}` (keys and values via
/// `repr`). The `repr`-reentrancy guard is managed by the caller.
async fn render_mapping(
    state: &mut InterpreterState,
    entries: &IndexMap<crate::value::ValueKey, Value>,
    tools: &Tools,
) -> Result<String, EvalError> {
    let mut out = String::from("{");
    let mut first = true;
    for (k, v) in entries {
        if !first {
            out.push_str(", ");
        }
        first = false;
        out.push_str(&render(state, &k.to_value(), RenderMode::Repr, tools).await?);
        out.push_str(": ");
        out.push_str(&render(state, v, RenderMode::Repr, tools).await?);
    }
    out.push('}');
    Ok(out)
}

/// Render a `@dataclass` instance as `ClassName(field=value, ...)` per
/// CPython's synthesized `__repr__`. Recurses on nested dataclass
/// children so `Outer(name='x', inner=Inner(value=1))` formats
/// fully — not `Outer(name='x', inner=<Inner object>)`.
///
/// Sync because dataclass-synthesized repr never calls user methods —
/// it walks the static field list and uses the stateless `repr()`
/// fallback for nested instances (which itself uses `<ClassName
/// object>` for non-dataclass instances; the slot-dispatch async path
/// is reserved for direct render entry).
fn render_dataclass(
    state: &InterpreterState,
    class_name: &str,
    inst: &crate::value::InstanceValue,
    fields: &[DataclassField],
) -> String {
    let mut out = String::new();
    out.push_str(class_name);
    out.push('(');
    let mut first = true;
    for field in fields.iter().filter(|f| f.repr && !f.init_only) {
        if !first {
            out.push_str(", ");
        }
        first = false;
        out.push_str(&field.name);
        out.push('=');
        let field_value = inst.fields.lock().get(&field.name).cloned().unwrap_or(Value::None);
        let rendered = match &field_value {
            Value::Instance(nested) => state
                .classes
                .get(&nested.class_name)
                .and_then(|nested_class| nested_class.dataclass_fields.clone())
                .map_or_else(
                    || field_value.repr(),
                    |nested_fields| {
                        render_dataclass(state, &nested.class_name, nested, &nested_fields)
                    },
                ),
            _ => field_value.repr(),
        };
        out.push_str(&rendered);
    }
    out.push(')');
    out
}

pub(crate) fn ascii_escape(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let cp = c as u32;
        if c.is_ascii() {
            out.push(c);
        } else if cp < 0x100 {
            // CPython escapes non-ASCII code points below 256 as `\xHH`.
            let _ = write!(out, "\\x{cp:02x}");
        } else if cp < 0x10000 {
            let _ = write!(out, "\\u{cp:04x}");
        } else {
            let _ = write!(out, "\\U{cp:08x}");
        }
    }
    out
}
