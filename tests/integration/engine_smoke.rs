// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Host-side smoke tests for the `Interpreter::execute` contract: stdout shape,
//! error propagation, configuration, value conversions, registered tools, and
//! cross-call state. Python-semantics behaviour lives in `parity_corpus/*.py`
//! and is exercised by the directory-walked parity runner; this file is what
//! Python can't model.

use std::collections::HashMap;

use interpretthis::{
    Interpreter, InterpreterConfig, InterpreterDeps, InterpreterResponse, KwargsExt,
    ToolDefinition, Tools, Value, ValueKey, shared_list,
};

use crate::common::{assert_error, assert_output};

fn interpreter() -> Interpreter {
    Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default())
}

fn no_tools() -> Tools {
    Tools::new()
}

fn err_msg(resp: &InterpreterResponse) -> String {
    resp.error.as_ref().map(ToString::to_string).unwrap_or_default()
}

// --- Execute contract ---

#[tokio::test]
async fn engine_execute_simple_assignment() {
    // No print — the contract here is that `x = 42` runs without error.
    let interp = interpreter();
    let resp = interp.execute("x = 42", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
}

#[tokio::test]
async fn engine_print_writes_to_stdout() {
    // Pins the host-observable channel for `print(...)`: any output must land
    // on `resp.stdout`, not bubble to the real process stdout.
    let interp = interpreter();
    let resp = interp.execute("print('hello world')", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert!(resp.stdout.contains("hello world"));
}

#[tokio::test]
async fn engine_default_config_executes() {
    let interp = interpreter();
    let resp = interp.execute("print('works')", &no_tools(), HashMap::new()).await;
    assert!(resp.is_ok());
    assert_eq!(resp.stdout.trim(), "works");
}

// --- Response API ---

#[tokio::test]
async fn engine_response_is_ok_on_success() {
    let interp = interpreter();
    let resp = interp.execute("print('done')", &no_tools(), HashMap::new()).await;
    assert!(resp.is_ok());
    assert!(resp.result().is_ok());
}

#[tokio::test]
async fn engine_response_is_err_on_raise() {
    let interp = interpreter();
    let resp = interp.execute("raise ValueError('oops')", &no_tools(), HashMap::new()).await;
    assert!(!resp.is_ok());
    assert!(resp.result().is_err());
}

// --- Cross-call state ---

#[tokio::test]
async fn engine_state_persists_across_execute_calls() {
    let interp = interpreter();
    let resp1 = interp.execute("x = 42", &no_tools(), HashMap::new()).await;
    assert!(resp1.error.is_none(), "error: {:?}", resp1.error);

    let resp2 = interp.execute("print(x)", &no_tools(), HashMap::new()).await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert_eq!(resp2.stdout.trim(), "42");
}

#[tokio::test]
async fn engine_state_get_variable() {
    let interp = interpreter();
    let resp = interp.execute("x = 42\ny = 'hello'", &no_tools(), HashMap::new()).await;
    assert!(resp.is_ok());

    assert_eq!(interp.get_variable("x"), Some(Value::Int(42)));
    assert_eq!(interp.get_variable("y"), Some(Value::String("hello".into())));
    assert_eq!(interp.get_variable("nonexistent"), None);
}

// --- Variables map ---

#[tokio::test]
async fn engine_variables_map_accepts_json_value() {
    let interp = interpreter();
    let config = Value::from_json(serde_json::json!({
        "name": "test",
        "count": 5,
        "items": [1, 2, 3]
    }));
    let variables = HashMap::from([("config".to_string(), config)]);

    let resp = interp
        .execute(
            r#"
name = config["name"]
count = config["count"]
items = config["items"]
print(f"{name},{count},{len(items)}")
"#,
            &no_tools(),
            variables,
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "test,5,3");
}

#[tokio::test]
async fn engine_variables_map_accepts_from_impls() {
    let interp = interpreter();
    let variables = HashMap::from([
        ("name".to_string(), "Alice".into()),
        ("age".to_string(), 30i64.into()),
        ("active".to_string(), true.into()),
    ]);
    let resp = interp.execute("print(f'{name},{age},{active}')", &no_tools(), variables).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "Alice,30,True");
}

// --- Registered tools ---

#[tokio::test]
async fn engine_registered_tool_via_deps() {
    let tools = Tools::new().with(
        "double",
        ToolDefinition::from_fn(|kwargs| async move {
            let n = kwargs.get_int("n").unwrap_or(0);
            Ok(Value::Int(n * 2))
        }),
    );
    let interp = Interpreter::new(InterpreterDeps { tools }, InterpreterConfig::default());

    // No tools passed to execute — uses the registered set on the Interpreter.
    let resp =
        interp.execute("result = double(n=21)\nprint(result)", &Tools::new(), HashMap::new()).await;
    assert!(resp.is_ok(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "42");
}

#[tokio::test]
async fn engine_registered_tools_merge_with_execute_tools() {
    let registered = Tools::new().with(
        "registered",
        ToolDefinition::from_fn(|_kwargs| async move { Ok(Value::String("from_builder".into())) }),
    );
    let interp =
        Interpreter::new(InterpreterDeps { tools: registered }, InterpreterConfig::default());

    let extra = Tools::new().with(
        "extra",
        ToolDefinition::from_fn(|_kwargs| async move { Ok(Value::String("from_execute".into())) }),
    );

    let resp = interp
        .execute("a = registered()\nb = extra()\nprint(f'{a},{b}')", &extra, HashMap::new())
        .await;
    assert!(resp.is_ok(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "from_builder,from_execute");
}

// --- Host-side protections (divergent from CPython) ---

/// CPython 3.12 allows `del print` at module scope (the next `print` call then
/// raises NameError because the shadow is gone). Our sandbox refuses the `del`
/// itself so the call site can't accidentally rebind builtins it relies on.
/// Track as a deliberate divergence — see `CONFORMANCE.md`.
#[tokio::test]
async fn engine_delete_builtin_name_rejected() {
    let interp = interpreter();
    let resp = interp.execute("del print", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "deleting a protected builtin should error");
}

#[tokio::test]
async fn engine_return_at_top_level_does_not_panic() {
    let interp = interpreter();
    // `return` outside a function is invalid Python; the contract here is that
    // our evaluator surfaces an error rather than panicking.
    let resp = interp.execute("x = 1\nreturn x", &no_tools(), HashMap::new()).await;
    let _ = resp;
}

// --- Value conversions: From impls ---

#[test]
fn engine_value_from_bool() {
    let v: Value = true.into();
    assert!(matches!(v, Value::Bool(true)));
}

#[test]
fn engine_value_from_i64() {
    let v: Value = 42i64.into();
    assert!(matches!(v, Value::Int(42)));
}

#[test]
fn engine_value_from_i32() {
    let v: Value = 7i32.into();
    assert!(matches!(v, Value::Int(7)));
}

#[test]
fn engine_value_from_f64() {
    let v: Value = 2.5f64.into();
    match v {
        Value::Float(f) => assert!((f - 2.5).abs() < f64::EPSILON),
        other => panic!("expected Float, got {other:?}"),
    }
}

#[test]
fn engine_value_from_string() {
    let v: Value = String::from("hello").into();
    assert!(matches!(v, Value::String(s) if s == "hello"));
}

#[test]
fn engine_value_from_str() {
    let v: Value = "world".into();
    assert!(matches!(v, Value::String(s) if s == "world"));
}

#[test]
fn engine_value_from_vec() {
    let v: Value = vec![Value::Int(1), Value::Int(2)].into();
    match v {
        Value::List(items) => assert_eq!(items.lock().len(), 2),
        other => panic!("expected List, got {other:?}"),
    }
}

#[test]
fn engine_value_from_option_some() {
    let v: Value = Some(42i64).into();
    assert!(matches!(v, Value::Int(42)));
}

#[test]
fn engine_value_from_option_none() {
    let v: Value = Option::<i64>::None.into();
    assert!(matches!(v, Value::None));
}

// --- Value conversions: JSON round-trip ---

#[test]
fn engine_value_from_json_null() {
    let v = Value::from_json(serde_json::Value::Null);
    assert!(matches!(v, Value::None));
}

#[test]
fn engine_value_from_json_bool() {
    let v = Value::from_json(serde_json::json!(true));
    assert!(matches!(v, Value::Bool(true)));
}

#[test]
fn engine_value_from_json_integer() {
    let v = Value::from_json(serde_json::json!(42));
    assert!(matches!(v, Value::Int(42)));
}

#[test]
fn engine_value_from_json_float() {
    let v = Value::from_json(serde_json::json!(2.5));
    match v {
        Value::Float(f) => assert!((f - 2.5).abs() < f64::EPSILON),
        other => panic!("expected Float, got {other:?}"),
    }
}

#[test]
fn engine_value_from_json_string() {
    let v = Value::from_json(serde_json::json!("hello"));
    assert!(matches!(v, Value::String(s) if s == "hello"));
}

#[test]
fn engine_value_from_json_array() {
    let v = Value::from_json(serde_json::json!([1, "two", null]));
    match v {
        Value::List(items) => {
            let snapshot = items.lock().clone();
            assert_eq!(snapshot.len(), 3);
            assert!(matches!(snapshot[0], Value::Int(1)));
            assert!(matches!(&snapshot[1], Value::String(s) if s == "two"));
            assert!(matches!(snapshot[2], Value::None));
        }
        other => panic!("expected List, got {other:?}"),
    }
}

#[test]
fn engine_value_from_json_object() {
    let v = Value::from_json(serde_json::json!({"name": "Alice", "age": 30}));
    match v {
        Value::Dict(map) => {
            assert_eq!(map.len(), 2);
            assert!(
                matches!(map.get(&ValueKey::String("name".into())), Some(Value::String(s)) if s == "Alice")
            );
            assert!(matches!(map.get(&ValueKey::String("age".into())), Some(Value::Int(30))));
        }
        other => panic!("expected Dict, got {other:?}"),
    }
}

#[test]
fn engine_value_from_json_nested() {
    let v = Value::from_json(serde_json::json!({
        "users": [{"name": "Bob", "active": true}],
        "count": 1
    }));
    match v {
        Value::Dict(map) => {
            assert_eq!(map.len(), 2);
            match map.get(&ValueKey::String("users".into())) {
                Some(Value::List(users)) => {
                    let snapshot = users.lock().clone();
                    assert_eq!(snapshot.len(), 1);
                    assert!(matches!(&snapshot[0], Value::Dict(_)));
                }
                other => panic!("expected List for 'users', got {other:?}"),
            }
        }
        other => panic!("expected Dict, got {other:?}"),
    }
}

#[test]
fn engine_value_to_json_primitives() {
    assert_eq!(Value::None.to_json(), serde_json::Value::Null);
    assert_eq!(Value::Bool(true).to_json(), serde_json::json!(true));
    assert_eq!(Value::Int(42).to_json(), serde_json::json!(42));
    assert_eq!(Value::String("hi".into()).to_json(), serde_json::json!("hi"));
}

#[test]
fn engine_value_to_json_collections() {
    let list = Value::List(shared_list(vec![Value::Int(1), Value::String("two".into())]));
    assert_eq!(list.to_json(), serde_json::json!([1, "two"]));

    let mut map = indexmap::IndexMap::new();
    map.insert(ValueKey::String("a".into()), Value::Int(1));
    let dict = Value::Dict(map);
    assert_eq!(dict.to_json(), serde_json::json!({"a": 1}));
}

#[test]
fn engine_value_json_round_trip() {
    let original = serde_json::json!({
        "name": "test",
        "values": [1, 2.5, true, null, "hello"],
        "nested": {"inner": [3, 4]}
    });
    let value = Value::from_json(original.clone());
    let back = value.to_json();
    assert_eq!(original, back);
}

// --- Value accessor methods ---

#[test]
fn engine_value_as_str() {
    assert_eq!(Value::String("hello".into()).as_str(), Some("hello"));
    assert_eq!(Value::Int(42).as_str(), None);
    assert_eq!(Value::None.as_str(), None);
}

#[test]
fn engine_value_as_int() {
    assert_eq!(Value::Int(42).as_int(), Some(42));
    assert_eq!(Value::String("hi".into()).as_int(), None);
    assert_eq!(Value::Bool(true).as_int(), None);
}

#[test]
fn engine_value_as_float() {
    assert_eq!(Value::Float(2.5).as_float(), Some(2.5));
    // Int promotes to float — the as_float contract is "viewable as a float".
    assert_eq!(Value::Int(5).as_float(), Some(5.0));
    assert_eq!(Value::String("hi".into()).as_float(), None);
}

#[test]
fn engine_value_as_bool() {
    assert_eq!(Value::Bool(true).as_bool(), Some(true));
    assert_eq!(Value::Bool(false).as_bool(), Some(false));
    assert_eq!(Value::Int(1).as_bool(), None);
}

#[test]
fn engine_value_as_list() {
    let list = Value::List(shared_list(vec![Value::Int(1), Value::Int(2)]));
    assert_eq!(list.as_list().unwrap().len(), 2);
    assert!(Value::Int(1).as_list().is_none());
}

#[test]
fn engine_value_as_dict() {
    let mut map = indexmap::IndexMap::new();
    map.insert(ValueKey::String("a".into()), Value::Int(1));
    let dict = Value::Dict(map);
    assert_eq!(dict.as_dict().unwrap().len(), 1);
    assert!(Value::Int(1).as_dict().is_none());
}

#[test]
fn engine_value_try_into_string() {
    let v = Value::String("hello".into());
    assert_eq!(v.try_into_string().unwrap(), "hello");

    let v = Value::Int(42);
    assert!(v.try_into_string().is_err());
}

#[test]
fn engine_value_try_into_list() {
    let v = Value::List(shared_list(vec![Value::Int(1)]));
    assert_eq!(v.try_into_list().unwrap().len(), 1);

    let v = Value::String("hi".into());
    assert!(v.try_into_list().is_err());
}

// --- Value PartialEq ---

#[test]
fn engine_value_partial_eq() {
    assert_eq!(Value::None, Value::None);
    assert_eq!(Value::Bool(true), Value::Bool(true));
    assert_ne!(Value::Bool(true), Value::Bool(false));
    assert_eq!(Value::Int(42), Value::Int(42));
    assert_ne!(Value::Int(1), Value::Int(2));
    assert_eq!(Value::Float(2.5), Value::Float(2.5));
    assert_eq!(Value::String("hello".into()), Value::String("hello".into()));
    assert_ne!(Value::String("a".into()), Value::String("b".into()));
    assert_eq!(
        Value::List(shared_list(vec![Value::Int(1), Value::Int(2)])),
        Value::List(shared_list(vec![Value::Int(1), Value::Int(2)]))
    );
    assert_ne!(Value::Int(42), Value::String("42".into()));
}

// --- Sandbox namespace + import policy ---

#[tokio::test]
async fn collections_requires_import() {
    // `collections` is not auto-imported into the sandbox namespace, so a bare
    // reference must raise (NameError shape on our side). Sandbox-policy check
    // rather than a parity one — CPython without an explicit import would
    // also raise, but for a different reason. See CONFORMANCE.md → Import allowlist.
    assert_error("print(collections.Counter('abc'))").await;
}

#[tokio::test]
async fn bare_math_namespace_is_name_error() {
    let interp = interpreter();
    let resp = interp.execute("print(math.sqrt(9))", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected NameError, got stdout: {:?}", resp.stdout);
    assert!(err_msg(&resp).contains("math"), "error should name 'math': {}", err_msg(&resp));
}

#[tokio::test]
async fn bare_collections_namespace_is_name_error() {
    let interp = interpreter();
    let resp =
        interp.execute("print(collections.Counter(\"aabbc\"))", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected NameError, got stdout: {:?}", resp.stdout);
    assert!(
        err_msg(&resp).contains("collections"),
        "error should name 'collections': {}",
        err_msg(&resp)
    );
}

#[tokio::test]
async fn bare_json_namespace_auto_imported() {
    // Divergence from CPython: we expose `json` without an `import` line so
    // sandbox users can reach it directly. CPython would raise NameError here.
    let interp = interpreter();
    let resp = interp.execute("print(json.dumps({\"a\": 1}))", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "{\"a\": 1}");
}

#[tokio::test]
async fn bare_re_namespace_auto_imported() {
    let interp = interpreter();
    let resp =
        interp.execute(r#"print(re.findall(r"\d+", "a1b22"))"#, &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "['1', '22']");
}

#[tokio::test]
async fn bare_datetime_namespace_auto_imported() {
    let interp = interpreter();
    let resp =
        interp.execute("print(datetime.date(2026, 1, 1))", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "2026-01-01");
}

#[tokio::test]
async fn eval_call_blocked_cleanly() {
    // Security boundary: `eval` blocked. Error must mention `eval` and must
    // not be double-wrapped (`name 'name '...'`) — a prior regression caused
    // by the validator re-running NameError formatting on its own message.
    let interp = interpreter();
    let resp = interp.execute("print(eval(\"2+2\"))", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "eval should be blocked, got stdout: {:?}", resp.stdout);
    let msg = err_msg(&resp);
    assert!(!msg.contains("name 'name '"), "error should not be double-wrapped: {msg}");
    assert!(msg.contains("eval"), "error should mention eval: {msg}");
}

#[tokio::test]
async fn exec_call_blocked_cleanly() {
    let interp = interpreter();
    let resp = interp.execute("exec(\"x=5\")\nprint(x)", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "exec should be blocked, got stdout: {:?}", resp.stdout);
    let msg = err_msg(&resp);
    assert!(!msg.contains("name 'name '"), "error should not be double-wrapped: {msg}");
    assert!(msg.contains("exec"), "error should mention exec: {msg}");
}

// --- Error diagnostic shape (customer-reported) ---
//
// Hosts typically persist `InterpreterResponse::error.to_string()`
// as the `errorMessage` field on the in-band SSE `execution_stop` frame, and
// downstream agent loops feed that string back into a planner LLM for
// self-correction. When the error has no line / variable-name context the
// planner can't pin which expression blew up — for a multi-statement script
// that means the loop runs blind. These tests pin the contract that the
// rendered error carries at least a line number.

#[tokio::test]
async fn engine_runtime_error_carries_line_number_single_statement() {
    // Reproducer from the customer report: `x = "hello"; x()`. The call
    // evaluator raises `TypeError("'str' object is not callable")` — that
    // shape is fine, but the message MUST surface where in the source the
    // failure landed so an agent loop can self-correct.
    let interp = interpreter();
    let resp = interp
        .execute(
            r#"x = "hello"
x()"#,
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_some(), "expected error, got stdout: {:?}", resp.stdout);
    let msg = err_msg(&resp);
    assert!(msg.contains("not callable"), "error should still name what went wrong: {msg}");
    assert!(
        msg.contains("line 2"),
        "error must include source line so agent loops can self-correct: {msg}"
    );
}

#[tokio::test]
async fn engine_runtime_error_points_at_the_offending_line() {
    // Multi-statement script. The failing call is on line 4 — the message
    // must point there, not at line 1 / the first statement / nowhere.
    let interp = interpreter();
    let resp = interp
        .execute(
            r#"a = 1
b = 2
c = "hello"
c()"#,
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_some(), "expected error, got stdout: {:?}", resp.stdout);
    let msg = err_msg(&resp);
    assert!(
        msg.contains("line 4"),
        "error must point at the offending line (4), not the file or nothing: {msg}"
    );
}

#[tokio::test]
async fn engine_dict_get_as_key_fn_either_works_or_reports_line() {
    // Customer-reported Bug 1: `max(d, key=d.get)` errors with
    // `'str' object is not callable` because the bound-method access on a
    // dict returns a method-marker sentinel (a String) instead of a
    // first-class callable. Either (a) we match CPython and the call
    // succeeds, or (b) we still error but the error names the source
    // line so the planner can rewrite to `key=lambda k: d[k]`.
    let interp = interpreter();
    let resp = interp
        .execute(
            r"monthly_data = {'A': 1, 'B': 2, 'C': 3}
highest = max(monthly_data, key=monthly_data.get)
print(highest)",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    if resp.error.is_none() {
        // Path (a): bound-method semantics fixed.
        assert_eq!(resp.stdout.trim(), "C");
    } else {
        // Path (b): still errors — but the message must point at line 2.
        let msg = err_msg(&resp);
        assert!(
            msg.contains("line 2"),
            "if max(d, key=d.get) still errors, the message must include the source line so agent loops can self-correct: {msg}"
        );
    }
}

#[tokio::test]
async fn engine_lambda_body_error_stamps_lambda_line() {
    // `f = lambda x: 1 / 0; f(5)` — the failure is in the lambda body
    // (line 1), not at the call site (line 2). The stamp must name the
    // lambda body so the agent loop can rewrite the lambda, not the
    // call site.
    let interp = interpreter();
    let resp = interp.execute("f = lambda x: 1 / 0\nf(5)", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected error, got stdout: {:?}", resp.stdout);
    let msg = err_msg(&resp);
    assert!(
        msg.contains("line 1"),
        "error must point at the lambda body (line 1), not the call site: {msg}"
    );
}

#[tokio::test]
async fn engine_function_body_error_stamps_inner_line() {
    // Function body has an error on line 3. The stamp must point there,
    // not at the outer line where the function is called (line 4).
    let interp = interpreter();
    let resp = interp
        .execute("def f():\n    y = 1\n    return y / 0\nf()", &no_tools(), HashMap::new())
        .await;
    assert!(resp.error.is_some(), "expected error, got stdout: {:?}", resp.stdout);
    let msg = err_msg(&resp);
    assert!(
        msg.contains("line 3"),
        "error must point at the failing return inside f (line 3), not the call site: {msg}"
    );
}

#[tokio::test]
async fn engine_recursion_works_at_realistic_depth() {
    // Each Python frame costs hundreds of KB of native stack today (every
    // `call_user_function` awaits across `execute_body` → `eval_stmt`
    // → `eval_expr` → `eval_call` → `call_user_function`, with each
    // Box::pin holding the full match arm state of eval_stmt/eval_expr).
    // Default test threads (~2 MB) currently handle a few levels; a
    // 16 MB production stack handles more. Pin a small depth so a
    // future regression (large new match-arm state) fails loud.
    //
    // Architectural fix: move match-arm selection outside the Pin<Box>
    // in eval_stmt/eval_expr (tracked separately).
    let interp = interpreter();
    let resp = interp
        .execute(
            "def f(n):\n    if n <= 0:\n        return 0\n    return f(n - 1) + 1\nprint(f(3))",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "3-level recursion must work on default stack: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "3");
}

#[tokio::test]
async fn engine_sentinel_lookalike_string_is_not_callable() {
    // Round 4 refactor: builtin/tool/class-method/exception bare names
    // are now typed Value variants. A user variable whose value
    // happens to MATCH an old sentinel string (e.g. assigning
    // "__builtin__print" by hand) must NOT become callable. Previous
    // sentinel-string dispatch would route this through try_builtin
    // and silently print -- a real collision risk.
    let interp = interpreter();
    let resp =
        interp.execute("f = '__builtin__print'\nf('hello')", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "string variable must not dispatch as a builtin");
    let msg = err_msg(&resp);
    assert!(
        msg.contains("'f' is not callable") || msg.contains("'str' object is not callable"),
        "must error with a callable-type complaint: {msg}"
    );
}

#[tokio::test]
async fn engine_lambda_survives_state_round_trip() {
    // Define a lambda, export+import state, call it. Lambdas were
    // previously skipped on export and the lambda_bodies map was
    // reset on import, so calling a persisted lambda silently
    // errored. Round 3 adds source on LambdaDef + re-parse during
    // import_state.
    let interp = interpreter();
    let resp = interp.execute("f = lambda x: x * 3", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "definition should succeed: {:?}", resp.error);

    let exported = interp.export_state().expect("export should succeed");

    // Fresh interpreter, import the state, then call the lambda.
    let interp2 = interpreter();
    interp2.import_state(&exported).expect("import should succeed");
    let call_resp = interp2.execute("print(f(7))", &no_tools(), HashMap::new()).await;
    assert!(call_resp.error.is_none(), "cross-execute call: {:?}", call_resp.error);
    assert_eq!(call_resp.stdout.trim(), "21");
}

#[tokio::test]
async fn engine_cross_execute_function_error_stamps_defined_line() {
    // Persisted function: defined in execute() #1, called in #2. The
    // function's body AST byte offsets point into the FIRST execute's
    // source, not the second one's. Without per-body source tracking
    // the stamp would either be wrong or fall back to line 1 of the
    // second execute. With body_source_stack pushing func_def.source
    // before recursion, the stamp resolves against the right source.
    let interp = interpreter();
    let resp1 =
        interp.execute("def divide(x):\n    return x / 0", &no_tools(), HashMap::new()).await;
    assert!(resp1.error.is_none(), "definition should succeed: {:?}", resp1.error);
    let resp2 = interp.execute("divide(5)", &no_tools(), HashMap::new()).await;
    assert!(resp2.error.is_some(), "expected error, got stdout: {:?}", resp2.stdout);
    let msg = err_msg(&resp2);
    assert!(
        msg.contains("line 2"),
        "error must point at the failing line inside the persisted function (line 2 of its def), not at line 1 of the call execute: {msg}"
    );
}

// --- CPython error-wording parity (strict prefix pins) ---
//
// CPython 3.12 renders runtime errors as `TypeError: <msg>`,
// `NameError: name '<n>' is not defined`, `ValueError: <msg>`,
// `AttributeError: <msg>`, `AssertionError`, `RecursionError: ...`,
// `RuntimeError: <msg>`, `SyntaxError: <msg>`. The host
// surfaces this string to a planner LLM that was trained on CPython
// tracebacks; mis-cased or rephrased prefixes drift the LLM into
// repair attempts that don't fit our error shape. These tests pin
// the prefixes byte-for-byte.

#[tokio::test]
async fn engine_typeerror_prefix_matches_cpython() {
    let interp = interpreter();
    let resp = interp.execute("1 + 'x'", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected error");
    let msg = err_msg(&resp);
    assert!(msg.starts_with("TypeError: "), "must begin with CPython prefix `TypeError: `: {msg}");
}

#[tokio::test]
async fn engine_nameerror_prefix_matches_cpython() {
    let interp = interpreter();
    let resp = interp.execute("undefined_name", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected error");
    let msg = err_msg(&resp);
    assert!(
        msg.starts_with("NameError: name '"),
        "must begin with CPython prefix `NameError: name '...`: {msg}"
    );
}

#[tokio::test]
async fn engine_valueerror_prefix_matches_cpython() {
    // Use a non-JSON failure mode whose CPython rendering is the bare
    // `ValueError:` prefix. (CPython renders `json.loads(bad)` as
    // `json.decoder.JSONDecodeError`, a ValueError SUBCLASS — that
    // case is pinned separately below.)
    let interp = interpreter();
    let resp = interp.execute("int('abc')", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected error");
    let msg = err_msg(&resp);
    assert!(
        msg.starts_with("ValueError: "),
        "must begin with CPython prefix `ValueError: `: {msg}"
    );
}

#[tokio::test]
async fn engine_jsondecodeerror_prefix_matches_cpython() {
    // CPython renders the qualified subclass name even though
    // JSONDecodeError IS-A ValueError. Pin the qualified form so a
    // planner LLM trained on CPython tracebacks sees the right shape.
    let interp = interpreter();
    let resp =
        interp.execute("import json\njson.loads('{not json')", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected error");
    let msg = err_msg(&resp);
    assert!(
        msg.starts_with("json.decoder.JSONDecodeError: "),
        "must begin with CPython qualified subclass name: {msg}"
    );
}

#[tokio::test]
async fn engine_attributeerror_prefix_matches_cpython() {
    let interp = interpreter();
    let resp = interp.execute("(1).nonexistent_attr", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected error");
    let msg = err_msg(&resp);
    assert!(
        msg.starts_with("AttributeError: "),
        "must begin with CPython prefix `AttributeError: `: {msg}"
    );
}

#[tokio::test]
async fn engine_assertionerror_prefix_matches_cpython() {
    let interp = interpreter();
    let resp = interp.execute("assert False, 'boom'", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected error");
    let msg = err_msg(&resp);
    assert!(
        msg.starts_with("AssertionError"),
        "must begin with CPython prefix `AssertionError`: {msg}"
    );
}

#[tokio::test]
async fn engine_syntaxerror_prefix_matches_cpython() {
    let interp = interpreter();
    let resp = interp.execute("def (", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected error");
    let msg = err_msg(&resp);
    assert!(
        msg.starts_with("SyntaxError: "),
        "must begin with CPython prefix `SyntaxError: `: {msg}"
    );
}

#[tokio::test]
async fn engine_zerodivision_prefix_matches_cpython() {
    let interp = interpreter();
    let resp = interp.execute("1 / 0", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected error");
    let msg = err_msg(&resp);
    assert!(
        msg.starts_with("ZeroDivisionError: "),
        "must begin with CPython prefix `ZeroDivisionError: `: {msg}"
    );
}

#[tokio::test]
async fn engine_recursionerror_prefix_matches_cpython() {
    // Use a tight recursion budget so the guard fires before each
    // async frame's native stack usage matters. Our Box::pin futures
    // still cost more native stack per level than CPython's frames
    // do, so the default 1000-level budget would crash the host
    // before tripping the guard. The variable-checkpoint refactor
    // raised the ceiling from ~5 to ~8 frames before host overflow;
    // we set this at 7 to leave headroom for CI thread-stack
    // variance (CI uses smaller default stack sizes than dev). The
    // value tracks the per-frame native-stack cost — bumping it
    // further requires another reduction in per-frame size (e.g.
    // shrinking the async-fn future or moving to a trampoline
    // evaluator).
    let mut config = InterpreterConfig::default();
    // Per-frame native stack cost grew with language-surface work; keep
    // the interpreter guard below the host stack ceiling.
    config.max_recursion_depth = 3;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, config);
    let resp = interp.execute("def f(): f()\nf()", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected error");
    let msg = err_msg(&resp);
    assert!(
        msg.starts_with("RecursionError: "),
        "must begin with CPython prefix `RecursionError: `: {msg}"
    );
}

#[tokio::test]
async fn engine_comprehension_element_error_stamped() {
    // A list comprehension whose element expression raises must
    // surface a line stamp. The comprehension itself is a single
    // statement so the stamp matches CPython's exact line for the
    // typical single-line case; multi-line comprehensions diverge
    // (we stamp the containing stmt's line, CPython points at the
    // element) — that's a documented limitation since the body of
    // an expression-level comprehension doesn't pass through
    // eval_stmt where the stamp lives.
    let interp = interpreter();
    let resp = interp
        .execute("data = [1, 2, 3]\nresult = [x() for x in data]", &no_tools(), HashMap::new())
        .await;
    assert!(resp.error.is_some(), "expected error, got stdout: {:?}", resp.stdout);
    let msg = err_msg(&resp);
    assert!(
        msg.contains("not callable") && msg.contains("line 2"),
        "comprehension element error must include type + line: {msg}"
    );
}

#[tokio::test]
async fn engine_json_loads_error_preserves_position() {
    // CPython's JSONDecodeError carries `line N column M`. serde_json's
    // Display already includes that information; this test pins that
    // the message reaches the host unchanged. If we ever wrap or rewrite
    // the error in a way that drops the position detail, this turns red.
    let interp = interpreter();
    let resp =
        interp.execute("import json\njson.loads('{not json')", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected error, got stdout: {:?}", resp.stdout);
    let msg = err_msg(&resp);
    assert!(
        msg.contains("line 1") && msg.contains("column"),
        "json.loads error must carry the parser's line/column for agent-loop self-correction: {msg}"
    );
}

// --- re module: documented host-side divergences from CPython ---

#[tokio::test]
async fn re_sub_backreference_uses_python_syntax() {
    // `re.sub` replacement now accepts CPython's `\1` / `\g<name>`
    // backref syntax. `$` is treated as a literal (CPython has no
    // special `$` substitution in replacements). The underlying
    // engine is still the Rust `regex` crate; the translator wraps
    // the Python form into `${1}` before passing to `replace_all`.
    assert_output(
        r#"import re
print(re.sub(r"(\w)", r"\1x", "ab"))"#,
        "axbx",
    )
    .await;
}

#[tokio::test]
async fn re_compile_is_unsupported() {
    // `re.compile` is not implemented — patterns are compiled on every call
    // site. CPython exposes a `Pattern` object; we have no equivalent type.
    assert_error("import re\nre.compile(r\"\\d+\")").await;
}
