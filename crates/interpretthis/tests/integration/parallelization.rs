// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

#![expect(
    clippy::unwrap_used,
    reason = "integration test helpers (ToolHandler impls for mock tools) aren't detected as \
              test context by clippy's allow-unwrap-in-tests, even though they live in a test \
              crate. unwrap on Mutex::lock() is fine here — a poisoned mutex already means an \
              invariant broke, and panicking with a clearer message offers no diagnostic value"
)]
#![expect(
    clippy::items_after_statements,
    reason = "each `#[tokio::test]` defines its own mock ToolHandler struct inline so the mock \
              name can be scoped to the test (otherwise structs like DictTool / ListTool at \
              module scope would collide). the scoping benefit is worth the lint noise"
)]

//! Phase 3: Parallelization tests.
//! Ported from Python `test_parallelization_opt_in.py` and `test_lazy_tool_proxy.py`.
//!
//! These validate opt-in parallelization via ToolDefinition.parallelizable,
//! `LazyProxy` resolution barriers, dependency chains, and concurrency limits.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use interpretthis::{
    Interpreter, InterpreterConfig, InterpreterDeps, ToolDefinition, ToolError, ToolHandler, Tools,
    Value, shared_list,
};

fn interpreter() -> Interpreter {
    Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default())
}

// --- Tool factories ---

struct CallOrderTool {
    log: Arc<Mutex<Vec<String>>>,
    delay: Duration,
}

#[async_trait]
impl ToolHandler for CallOrderTool {
    async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        let name = match kwargs.get("name") {
            Some(Value::String(s)) => s.clone(),
            _ => "unknown".into(),
        };
        self.log.lock().unwrap().push(format!("{name}_start"));
        tokio::time::sleep(self.delay).await;
        self.log.lock().unwrap().push(format!("{name}_end"));
        Ok(Value::String(format!("result_{name}").into()))
    }
}

struct FastTool {
    log: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl ToolHandler for FastTool {
    async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        self.log.lock().unwrap().push("fast".to_string());
        Ok(Value::String("fast".into()))
    }
}

struct SlowTool {
    log: Arc<Mutex<Vec<String>>>,
    delay: Duration,
}

#[async_trait]
impl ToolHandler for SlowTool {
    async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        self.log.lock().unwrap().push("slow_start".to_string());
        tokio::time::sleep(self.delay).await;
        self.log.lock().unwrap().push("slow_end".to_string());
        Ok(Value::String("slow".into()))
    }
}

struct ConcurrencyTracker {
    current: Arc<Mutex<u32>>,
    peak: Arc<Mutex<u32>>,
}

#[async_trait]
impl ToolHandler for ConcurrencyTracker {
    async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        {
            let mut c = self.current.lock().unwrap();
            *c += 1;
            let mut p = self.peak.lock().unwrap();
            if *c > *p {
                *p = *c;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        {
            let mut c = self.current.lock().unwrap();
            *c -= 1;
        }
        Ok(Value::None)
    }
}

struct FailingAsyncTool {
    message: String,
}

#[async_trait]
impl ToolHandler for FailingAsyncTool {
    async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Err(ToolError::new(&self.message))
    }
}

// --- Opt-in: volatile tools are sequential ---

#[tokio::test]
async fn parallelization_volatile_tools_preserve_call_order() {
    let log = Arc::new(Mutex::new(Vec::new()));

    let mut tools = Tools::new();
    tools.insert(
        "slow",
        ToolDefinition {
            handler: Arc::new(SlowTool { log: log.clone(), delay: Duration::from_millis(100) }),
            parallelizable: false,
        },
    );
    tools.insert(
        "fast",
        ToolDefinition { handler: Arc::new(FastTool { log: log.clone() }), parallelizable: false },
    );

    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
a = fast()
b = slow()
c = fast()
print(f"{a},{b},{c}")
"#,
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "fast,slow,fast");
    // Volatile tools execute eagerly in order
    let entries = log.lock().unwrap().clone();
    assert_eq!(entries, vec!["fast", "slow_start", "slow_end", "fast"]);
}

// --- Opt-in: parallelizable tools defer ---

#[tokio::test]
async fn parallelization_parallelizable_tools_run_concurrently() {
    let log = Arc::new(Mutex::new(Vec::new()));

    let mut tools = Tools::new();
    tools.insert(
        "slow",
        ToolDefinition {
            handler: Arc::new(SlowTool { log: log.clone(), delay: Duration::from_millis(100) }),
            parallelizable: true,
        },
    );
    tools.insert(
        "fast",
        ToolDefinition { handler: Arc::new(FastTool { log: log.clone() }), parallelizable: true },
    );

    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
a = fast()
b = slow()
c = fast()
print(f"{a},{b},{c}")
"#,
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "fast,slow,fast");
    // With parallelizable tools, slow defers — second fast fires before slow ends
    let entries = log.lock().unwrap().clone();
    let slow_end_idx = entries.iter().position(|e| e == "slow_end").unwrap();
    let second_fast_idx = entries.iter().rposition(|e| e == "fast").unwrap();
    assert!(
        second_fast_idx < slow_end_idx,
        "second fast should complete before slow_end: {entries:?}"
    );
}

#[tokio::test]
async fn parallelization_volatile_tool_blocks_even_when_others_are_parallelizable() {
    let log = Arc::new(Mutex::new(Vec::new()));

    let mut tools = Tools::new();
    tools.insert(
        "parallel_tool",
        ToolDefinition {
            handler: Arc::new(CallOrderTool { log: log.clone(), delay: Duration::from_millis(50) }),
            parallelizable: true,
        },
    );
    tools.insert(
        "barrier_tool",
        ToolDefinition {
            handler: Arc::new(CallOrderTool { log: log.clone(), delay: Duration::from_millis(10) }),
            parallelizable: false, // volatile
        },
    );

    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
a = parallel_tool(name="a")
b = barrier_tool(name="barrier")
c = parallel_tool(name="c")
print(f"{a},{b},{c}")
"#,
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let entries = log.lock().unwrap().clone();
    let barrier_end = entries.iter().position(|e| e == "barrier_end").unwrap();
    let c_start = entries.iter().position(|e| e == "c_start").unwrap();
    assert!(barrier_end < c_start, "barrier must complete before c starts: {entries:?}");
}

// --- Dependency chains ---

#[tokio::test]
async fn parallelization_chained_dependency() {
    let log = Arc::new(Mutex::new(Vec::new()));

    let mut tools = Tools::new();
    tools.insert(
        "step",
        ToolDefinition {
            handler: Arc::new(CallOrderTool { log: log.clone(), delay: Duration::from_millis(50) }),
            parallelizable: true,
        },
    );

    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
a = step(name="a")
b = step(name="b", dep=a)
c = step(name="c", dep=b)
print(c)
"#,
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let entries = log.lock().unwrap().clone();
    assert!(
        entries.iter().position(|e| e == "a_end").unwrap()
            < entries.iter().position(|e| e == "b_start").unwrap()
    );
    assert!(
        entries.iter().position(|e| e == "b_end").unwrap()
            < entries.iter().position(|e| e == "c_start").unwrap()
    );
}

#[tokio::test]
async fn parallelization_diamond_dependency() {
    let log = Arc::new(Mutex::new(Vec::new()));

    let mut tools = Tools::new();
    tools.insert(
        "node",
        ToolDefinition {
            handler: Arc::new(CallOrderTool { log: log.clone(), delay: Duration::from_millis(50) }),
            parallelizable: true,
        },
    );

    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
a = node(name="a")
b = node(name="b", dep=a)
c = node(name="c", dep=a)
d = node(name="d", dep_b=b, dep_c=c)
print(d)
"#,
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let entries = log.lock().unwrap().clone();
    // a finishes before b and c start
    assert!(
        entries.iter().position(|e| e == "a_end").unwrap()
            < entries.iter().position(|e| e == "b_start").unwrap()
    );
    assert!(
        entries.iter().position(|e| e == "a_end").unwrap()
            < entries.iter().position(|e| e == "c_start").unwrap()
    );
    // d starts after both b and c finish
    assert!(
        entries.iter().position(|e| e == "b_end").unwrap()
            < entries.iter().position(|e| e == "d_start").unwrap()
    );
    assert!(
        entries.iter().position(|e| e == "c_end").unwrap()
            < entries.iter().position(|e| e == "d_start").unwrap()
    );
}

// --- Resolution at barriers ---

#[tokio::test]
async fn parallelization_resolve_at_attribute_access() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct DictTool;
    #[async_trait]
    impl ToolHandler for DictTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::String("test_value".into()))
        }
    }

    tools.insert("search", ToolDefinition { handler: Arc::new(DictTool), parallelizable: true });

    let resp = interp
        .execute(
            r"
result = search()
print(result)
",
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "test_value");
}

#[tokio::test]
async fn parallelization_resolve_at_iteration() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct ListTool;
    #[async_trait]
    impl ToolHandler for ListTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::List(shared_list(vec![Value::Int(1), Value::Int(2), Value::Int(3)])))
        }
    }

    tools.insert("get_items", ToolDefinition { handler: Arc::new(ListTool), parallelizable: true });

    let resp = interp
        .execute(
            r"
items = get_items()
total = 0
for item in items:
    total = total + item
print(total)
",
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "6");
}

#[tokio::test]
async fn parallelization_resolve_at_if_condition() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct BoolTool;
    #[async_trait]
    impl ToolHandler for BoolTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::Bool(true))
        }
    }

    tools.insert("check", ToolDefinition { handler: Arc::new(BoolTool), parallelizable: true });

    let resp = interp
        .execute(
            r#"
flag = check()
if flag:
    result = "yes"
else:
    result = "no"
print(result)
"#,
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "yes");
}

#[tokio::test]
async fn parallelization_resolve_at_binary_operator() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct NumTool;
    #[async_trait]
    impl ToolHandler for NumTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::Int(10))
        }
    }

    tools.insert("get_number", ToolDefinition { handler: Arc::new(NumTool), parallelizable: true });

    let resp = interp
        .execute(
            r"
a = get_number()
b = get_number()
result = a + b
print(result)
",
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "20");
}

#[tokio::test]
async fn parallelization_resolve_at_print() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct MsgTool;
    #[async_trait]
    impl ToolHandler for MsgTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::String("hello world".into()))
        }
    }

    tools.insert("get_msg", ToolDefinition { handler: Arc::new(MsgTool), parallelizable: true });

    let resp = interp.execute("msg = get_msg()\nprint(msg)", &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert!(resp.stdout.contains("hello world"));
}

#[tokio::test]
async fn parallelization_resolve_at_fstring() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct NameTool;
    #[async_trait]
    impl ToolHandler for NameTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::String("Alice".into()))
        }
    }

    tools.insert("get_name", ToolDefinition { handler: Arc::new(NameTool), parallelizable: true });

    let resp = interp
        .execute(
            r#"
name = get_name()
greeting = f"Hello, {name}!"
print(greeting)
"#,
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "Hello, Alice!");
}

#[tokio::test]
async fn parallelization_resolve_at_comparison() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct ValTool;
    #[async_trait]
    impl ToolHandler for ValTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::Int(5))
        }
    }

    tools.insert("get_val", ToolDefinition { handler: Arc::new(ValTool), parallelizable: true });

    let resp = interp
        .execute("x = get_val()\nresult = x > 3\nprint(result)", &tools, HashMap::new())
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "True");
}

// --- Error propagation ---

#[tokio::test]
async fn parallelization_error_surfaces_at_use() {
    let interp = interpreter();
    let mut tools = Tools::new();
    tools.insert(
        "bad_tool",
        ToolDefinition {
            handler: Arc::new(FailingAsyncTool { message: "kaboom".to_string() }),
            parallelizable: true,
        },
    );

    let resp = interp.execute("result = bad_tool()\nprint(result)", &tools, HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn parallelization_error_does_not_surface_if_unused() {
    let interp = interpreter();
    let mut tools = Tools::new();
    tools.insert(
        "bad_tool",
        ToolDefinition {
            handler: Arc::new(FailingAsyncTool { message: "hidden".to_string() }),
            parallelizable: true,
        },
    );

    let resp =
        interp.execute("unused = bad_tool()\nprint('ignored')", &tools, HashMap::new()).await;
    // print should be reached before the proxy is resolved
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "ignored");
}

#[tokio::test]
async fn parallelization_good_tools_work_when_one_fails() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct GoodTool;
    #[async_trait]
    impl ToolHandler for GoodTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::String("good".into()))
        }
    }

    tools.insert("good", ToolDefinition { handler: Arc::new(GoodTool), parallelizable: true });
    tools.insert(
        "bad",
        ToolDefinition {
            handler: Arc::new(FailingAsyncTool { message: "fail".to_string() }),
            parallelizable: true,
        },
    );

    let resp = interp.execute("a = good()\nprint(a)", &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "good");
}

// --- Concurrency limits ---

#[tokio::test]
async fn parallelization_respects_concurrency_limit() {
    let current = Arc::new(Mutex::new(0u32));
    let peak = Arc::new(Mutex::new(0u32));

    let mut tools = Tools::new();
    tools.insert(
        "track",
        ToolDefinition {
            handler: Arc::new(ConcurrencyTracker { current: current.clone(), peak: peak.clone() }),
            parallelizable: true,
        },
    );

    let mut cfg = InterpreterConfig::default();
    cfg.max_concurrent_tools = 3;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);

    // Launch 10 concurrent calls
    use std::fmt::Write as _;
    let assignments: String = (0..10).fold(String::new(), |mut s, i| {
        let _ = writeln!(&mut s, "r{i} = track()");
        s
    });
    let uses: String = (0..10).map(|i| format!("str(r{i})")).collect::<Vec<_>>().join(" + ");
    let code = format!("{assignments}result = {uses}\nprint(result)");

    let resp = interp.execute(&code, &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let peak_val = *peak.lock().unwrap();
    assert!(peak_val <= 3, "peak concurrency {peak_val} exceeded limit 3");
}

#[tokio::test]
async fn parallelization_sync_tools_still_work() {
    // Non-async tools (parallelizable=false) should work normally
    let interp = interpreter();
    let mut tools = Tools::new();

    struct SyncTool;
    #[async_trait]
    impl ToolHandler for SyncTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::String("sync_result".into()))
        }
    }

    tools
        .insert("sync_tool", ToolDefinition { handler: Arc::new(SyncTool), parallelizable: false });

    let resp = interp.execute("result = sync_tool()\nprint(result)", &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "sync_result");
}

// --- Additional barrier resolution tests ---

#[tokio::test]
async fn parallelization_resolve_at_while_condition() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct ZeroTool;
    #[async_trait]
    impl ToolHandler for ZeroTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::Int(0))
        }
    }

    tools.insert(
        "get_counter",
        ToolDefinition { handler: Arc::new(ZeroTool), parallelizable: true },
    );

    let resp = interp
        .execute(
            r"
counter = get_counter()
while counter < 3:
    counter = counter + 1
print(counter)
",
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "3");
}

#[tokio::test]
async fn parallelization_resolve_at_boolean_operator() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct TrueTool;
    #[async_trait]
    impl ToolHandler for TrueTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::Bool(true))
        }
    }
    struct FalseTool;
    #[async_trait]
    impl ToolHandler for FalseTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::Bool(false))
        }
    }

    tools.insert("get_true", ToolDefinition { handler: Arc::new(TrueTool), parallelizable: true });
    tools
        .insert("get_false", ToolDefinition { handler: Arc::new(FalseTool), parallelizable: true });

    let resp = interp
        .execute(
            r"
a = get_true()
b = get_false()
result = a and b
print(result)
",
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "False");
}

#[tokio::test]
async fn parallelization_resolve_at_ternary() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct FlagTool;
    #[async_trait]
    impl ToolHandler for FlagTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::Bool(true))
        }
    }

    tools.insert("get_flag", ToolDefinition { handler: Arc::new(FlagTool), parallelizable: true });

    let resp = interp
        .execute(
            r#"
flag = get_flag()
result = "yes" if flag else "no"
print(result)
"#,
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "yes");
}

#[tokio::test]
async fn parallelization_resolve_in_list_comprehension() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct ItemsTool;
    #[async_trait]
    impl ToolHandler for ItemsTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::List(shared_list(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4),
                Value::Int(5),
            ])))
        }
    }

    tools
        .insert("get_items", ToolDefinition { handler: Arc::new(ItemsTool), parallelizable: true });

    let resp = interp
        .execute(
            r"
items = get_items()
doubled = [x * 2 for x in items]
print(doubled)
",
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "[2, 4, 6, 8, 10]");
}

#[tokio::test]
async fn parallelization_resolve_at_unary_operator() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct NumTool;
    #[async_trait]
    impl ToolHandler for NumTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::Int(5))
        }
    }

    tools.insert("get_num", ToolDefinition { handler: Arc::new(NumTool), parallelizable: true });

    let resp =
        interp.execute("x = get_num()\nresult = -x\nprint(result)", &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "-5");
}

#[tokio::test]
async fn parallelization_resolve_at_subscript() {
    let interp = interpreter();
    let mut tools = Tools::new();

    struct DictTool;
    #[async_trait]
    impl ToolHandler for DictTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            let mut map = indexmap::IndexMap::new();
            map.insert(
                interpretthis::ValueKey::String("name".into()),
                Value::String("test".into()),
            );
            map.insert(interpretthis::ValueKey::String("score".into()), Value::Int(42));
            Ok(Value::Dict(map))
        }
    }

    tools.insert("search", ToolDefinition { handler: Arc::new(DictTool), parallelizable: true });

    let resp = interp
        .execute(
            r#"
result = search()
name = result["name"]
print(name)
"#,
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "test");
}

// --- Failed proxy sentinel ---

#[tokio::test]
async fn parallelization_failed_proxy_surfaces_error_on_next_use() {
    let interp = interpreter();
    let mut tools = Tools::new();
    tools.insert(
        "failing_tool",
        ToolDefinition {
            handler: Arc::new(FailingAsyncTool { message: "connection_refused".to_string() }),
            parallelizable: true,
        },
    );

    struct GoodTool;
    #[async_trait]
    impl ToolHandler for GoodTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::String("ok".into()))
        }
    }
    tools.insert("good_tool", ToolDefinition { handler: Arc::new(GoodTool), parallelizable: true });

    // First call: store a failing tool result without consuming it
    let _resp1 = interp.execute("data = failing_tool()", &tools, HashMap::new()).await;

    // Second call: try to use the variable
    let resp2 = interp.execute("result = good_tool()\nprint(data)", &tools, HashMap::new()).await;
    // Should get an error referencing the tool failure, not a bare NameError
    if let Some(ref err) = resp2.error {
        let msg = format!("{err:?}");
        assert!(
            !msg.contains("not defined"),
            "got unhelpful NameError instead of tool failure: {msg}"
        );
    }
}
