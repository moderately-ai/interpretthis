// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

#![expect(
    clippy::items_after_statements,
    reason = "per-test local mock ToolHandler structs trigger items_after_statements; the \
              scoping benefit of keeping them inside each test is worth the lint noise"
)]

//! Phase 2: Tool system tests.
//! Ported from Python `test_interpreter.py`, `test_tools_not_pickled.py`, `test_resolver.py`.
//!
//! These validate tool injection, resolution priority, name protection,
//! and that tools don't leak into serialized state.

use std::collections::HashMap;

use async_trait::async_trait;
use interpretthis::{
    Interpreter, InterpreterConfig, InterpreterDeps, KwargsExt, ToolDefinition, ToolError,
    ToolHandler, Tools, Value,
};

fn interpreter() -> Interpreter {
    Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default())
}

fn no_tools() -> Tools {
    Tools::new()
}

// --- Test tool implementations ---

struct EchoTool;

#[async_trait]
impl ToolHandler for EchoTool {
    async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        let text = kwargs.get("text").cloned().unwrap_or(Value::String("".into()));
        match text {
            Value::String(s) => Ok(Value::String(format!("echo: {s}").into())),
            other => Ok(Value::String(format!("echo: {other}").into())),
        }
    }
}

struct AddTool;

#[async_trait]
impl ToolHandler for AddTool {
    async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        let a = match kwargs.get("a") {
            Some(Value::Int(i)) => *i,
            _ => return Err(ToolError::new("missing 'a'")),
        };
        let b = match kwargs.get("b") {
            Some(Value::Int(i)) => *i,
            _ => return Err(ToolError::new("missing 'b'")),
        };
        Ok(Value::Int(a + b))
    }
}

struct FailingTool;

#[async_trait]
impl ToolHandler for FailingTool {
    async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        Err(ToolError::new("deliberate failure"))
    }
}

fn echo_tool() -> ToolDefinition {
    ToolDefinition::new(EchoTool)
}

fn add_tool() -> ToolDefinition {
    ToolDefinition::new(AddTool)
}

fn failing_tool() -> ToolDefinition {
    ToolDefinition::new(FailingTool)
}

// --- Tool calling ---

#[tokio::test]
async fn tool_call_custom_tool() {
    let interp = interpreter();
    let tools = Tools::new().with("echo", echo_tool());

    let resp =
        interp.execute("result = echo(text='hello')\nprint(result)", &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "echo: hello");
}

#[tokio::test]
async fn tool_call_tool_with_multiple_kwargs() {
    let interp = interpreter();
    let tools = Tools::new().with("add", add_tool());

    let resp =
        interp.execute("result = add(a=3, b=4)\nprint(result)", &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "7");
}

#[tokio::test]
async fn tool_error_propagates() {
    let interp = interpreter();
    let tools = Tools::new().with("fail", failing_tool());

    let resp = interp.execute("result = fail()", &tools, HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn tool_lambda_calling_tool() {
    let interp = interpreter();
    let tools = Tools::new().with("echo", echo_tool());

    let resp = interp
        .execute(
            r#"
fn = lambda t: echo(text=t)
result = fn("world")
print(result)
"#,
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "echo: world");
}

// --- Name protection ---

#[tokio::test]
async fn tool_assign_static_tool_name_should_fail() {
    let interp = interpreter();
    let resp = interp.execute("print = 42", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn tool_redefine_custom_tool_is_not_allowed() {
    let interp = interpreter();
    let tools = Tools::new().with("echo", echo_tool());

    let resp = interp.execute("def echo(x):\n    return x", &tools, HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[test]
#[should_panic(expected = "dangerous builtin")]
fn tool_cannot_provide_dangerous_tool_name() {
    // Tool name validation now happens at registration time (panics)
    let mut tools = Tools::new();
    tools.insert("eval", echo_tool());
}

// --- Tools not in state ---

#[tokio::test]
async fn tool_tools_not_in_state_keys() {
    let interp = interpreter();
    let tools = Tools::new().with("echo", echo_tool());

    let resp = interp.execute("result = echo(text='hi')", &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let keys = interp.state_keys();
    assert!(keys.contains(&"result".to_string()));
    assert!(!keys.contains(&"echo".to_string()), "tool should not be in state keys");
}

// --- Tool resolution priority ---

#[tokio::test]
async fn tool_static_tool_takes_priority_over_custom() {
    let interp = interpreter();
    let tools = Tools::new().with("len", echo_tool());

    let resp = interp.execute("result = len([1,2,3])\nprint(result)", &tools, HashMap::new()).await;
    if resp.error.is_none() {
        assert_eq!(resp.stdout.trim(), "3");
    }
}

#[tokio::test]
async fn tool_custom_tool_takes_priority_over_state_variable() {
    let interp = interpreter();
    let tools = Tools::new().with("my_func", echo_tool());

    let resp = interp
        .execute(
            r#"
result = my_func(text="from_tool")
print(result)
"#,
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "echo: from_tool");
}

#[tokio::test]
async fn tool_custom_tool_named_final_answer() {
    let interp = interpreter();

    struct CustomFinalAnswer;
    #[async_trait]
    impl ToolHandler for CustomFinalAnswer {
        async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            let output = kwargs.get("output").cloned().unwrap_or(Value::None);
            Ok(Value::String(format!("custom: {output}").into()))
        }
    }

    let tools = Tools::new().with("final_answer", ToolDefinition::new(CustomFinalAnswer));

    let resp = interp
        .execute("result = final_answer(output='test')\nprint(result)", &tools, HashMap::new())
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert!(resp.stdout.contains("custom:"));
}

// --- Ergonomics: ToolDefinition constructors ---

#[tokio::test]
async fn tool_definition_new_constructor() {
    let interp = interpreter();
    let tools = Tools::new().with("echo", ToolDefinition::new(EchoTool));

    let resp =
        interp.execute("result = echo(text='hi')\nprint(result)", &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "echo: hi");
}

// --- Ergonomics: from_fn closure tools ---

#[tokio::test]
async fn tool_from_closure() {
    let interp = interpreter();
    let tools = Tools::new().with(
        "greet",
        ToolDefinition::from_fn(|kwargs| async move {
            let name = kwargs.get_str("name").unwrap_or("world");
            Ok(Value::String(format!("hello, {name}!").into()))
        }),
    );

    let resp =
        interp.execute("result = greet(name='rust')\nprint(result)", &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "hello, rust!");
}

#[tokio::test]
async fn tool_from_closure_parallelizable() {
    let interp = interpreter();
    let tools = Tools::new().with(
        "compute",
        ToolDefinition::from_fn_parallel(|kwargs| async move {
            let n = kwargs.get_int("n").unwrap_or(0);
            Ok(Value::Int(n * 2))
        }),
    );

    let resp =
        interp.execute("result = compute(n=21)\nprint(result)", &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "42");
}

// --- Ergonomics: tools builder ---

#[tokio::test]
async fn tool_tools_builder_chaining() {
    let interp = interpreter();
    let t = Tools::new()
        .with("echo", ToolDefinition::new(EchoTool))
        .with("add", ToolDefinition::new(AddTool));

    let resp = interp
        .execute("a = echo(text='hi')\nb = add(a=1, b=2)\nprint(f'{a},{b}')", &t, HashMap::new())
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "echo: hi,3");
}

// --- Ergonomics: KwargsExt ---

#[tokio::test]
async fn tool_kwargs_require_str() {
    let interp = interpreter();
    let tools = Tools::new().with(
        "need_str",
        ToolDefinition::from_fn(|kwargs| async move {
            let val = kwargs.require_str("text")?;
            Ok(Value::String(format!("got: {val}").into()))
        }),
    );

    let resp = interp
        .execute("result = need_str(text='hello')\nprint(result)", &tools, HashMap::new())
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "got: hello");

    let resp2 = interp.execute("need_str()", &tools, HashMap::new()).await;
    assert!(resp2.error.is_some());
}

#[tokio::test]
async fn tool_kwargs_get_with_default() {
    let interp = interpreter();
    let tools = Tools::new().with(
        "with_default",
        ToolDefinition::from_fn(|kwargs| async move {
            let count = kwargs.get_int("count").unwrap_or(10);
            Ok(Value::Int(count))
        }),
    );

    let resp = interp
        .execute("result = with_default(count=5)\nprint(result)", &tools, HashMap::new())
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "5");

    let resp2 =
        interp.execute("result = with_default()\nprint(result)", &tools, HashMap::new()).await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert_eq!(resp2.stdout.trim(), "10");
}
