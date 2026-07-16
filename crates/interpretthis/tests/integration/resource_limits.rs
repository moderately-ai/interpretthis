// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

#![expect(
    clippy::unwrap_used,
    clippy::items_after_statements,
    reason = "integration-test helper impls aren't detected as test context by clippy's \
              in-tests allowlist, and per-test local struct definitions are the idiomatic \
              scoping for mock ToolHandlers"
)]

//! Resource-limit boundary tests: op counter, while-iteration cap, recursion
//! depth, wall-clock execution timeout, and concurrent-tool semaphore. Also
//! covers proxy-related stress concerns that show up at limit boundaries —
//! ordering, error propagation, falsy-value resolution, nested data — since
//! those classes of bug are easiest to expose with a many-call fan-out.

use std::{
    collections::HashMap,
    fmt::Write as _,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use interpretthis::{
    Interpreter, InterpreterConfig, InterpreterDeps, ToolDefinition, ToolError, ToolHandler, Tools,
    Value,
};

fn no_tools() -> Tools {
    Tools::new()
}

fn stress_interpreter() -> Interpreter {
    // Bigger budget than the default so the many-call stress tests run to
    // completion without tripping the op counter — the assertions here are
    // about concurrency/proxy correctness, not resource caps.
    let mut cfg = InterpreterConfig::default();
    cfg.max_operations = 500_000;
    cfg.max_while_iterations = 10_000;
    Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg)
}

// --- Tool fixtures used by the stress / proxy-behaviour cases ---

struct CountingTool {
    count: Arc<Mutex<u32>>,
}

#[async_trait]
impl ToolHandler for CountingTool {
    async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        let i = {
            // MutexGuard must drop before the .await to avoid holding a sync
            // lock across an await point.
            let mut c = self.count.lock().unwrap();
            *c += 1;
            match kwargs.get("i") {
                Some(Value::Int(v)) => *v,
                _ => i64::from(*c) - 1,
            }
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        Ok(Value::Int(i))
    }
}

struct ConcurrencyCountingTool {
    active: Arc<Mutex<u32>>,
    peak: Arc<Mutex<u32>>,
    total: Arc<Mutex<u32>>,
}

#[async_trait]
impl ToolHandler for ConcurrencyCountingTool {
    async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        let idx = {
            let mut t = self.total.lock().unwrap();
            let idx = *t;
            *t += 1;
            idx
        };
        {
            let mut a = self.active.lock().unwrap();
            *a += 1;
            let mut p = self.peak.lock().unwrap();
            if *a > *p {
                *p = *a;
            }
        }
        let delay = match kwargs.get("delay") {
            Some(Value::Float(d)) => Duration::from_secs_f64(*d),
            _ => Duration::from_millis(20),
        };
        tokio::time::sleep(delay).await;
        {
            let mut a = self.active.lock().unwrap();
            *a -= 1;
        }
        Ok(Value::Int(i64::from(idx)))
    }
}

struct MaybeFailTool;

#[async_trait]
impl ToolHandler for MaybeFailTool {
    async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let should_fail = matches!(kwargs.get("fail"), Some(Value::Bool(true)));
        if should_fail {
            Err(ToolError::new("deliberate_failure"))
        } else {
            Ok(Value::String("ok".into()))
        }
    }
}

struct FetchTool;

#[async_trait]
impl ToolHandler for FetchTool {
    async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let v = kwargs.get("v").cloned().unwrap_or(Value::Int(0));
        Ok(v)
    }
}

struct BoolTool {
    value: bool,
}

#[async_trait]
impl ToolHandler for BoolTool {
    async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(Value::Bool(self.value))
    }
}

struct IntTool {
    value: i64,
}

#[async_trait]
impl ToolHandler for IntTool {
    async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(Value::Int(self.value))
    }
}

struct EmptyStringTool;

#[async_trait]
impl ToolHandler for EmptyStringTool {
    async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(Value::String("".into()))
    }
}

// --- Operation counter ---

#[tokio::test]
async fn resource_limits_terminate_runaway_loop_via_op_counter() {
    let mut cfg = InterpreterConfig::default();
    cfg.max_operations = 100;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);
    let resp = interp
        .execute(
            r"
x = 0
for i in range(1000):
    x = x + 1
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_some());
    let err_msg = format!("{:?}", resp.error.unwrap());
    assert!(
        err_msg.contains("limit") || err_msg.contains("operation"),
        "error should mention limit: {err_msg}"
    );
}

#[tokio::test]
async fn resource_limits_normal_operations_within_default_budget() {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let resp = interp
        .execute(
            r"
total = 0
for i in range(100):
    total += i
print(total)
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
}

#[tokio::test]
async fn resource_limits_nested_loop_within_default_budget() {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let resp = interp
        .execute(
            r"
total = 0
for i in range(10):
    for j in range(10):
        total += i * j
print(total)
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
}

// --- While-iteration cap ---

#[tokio::test]
async fn resource_limits_terminate_while_loop_via_iteration_cap() {
    let mut cfg = InterpreterConfig::default();
    cfg.max_while_iterations = 50;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);
    let resp = interp
        .execute(
            r"
x = 0
while True:
    x += 1
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn resource_limits_while_loop_with_break_succeeds() {
    let mut cfg = InterpreterConfig::default();
    cfg.max_while_iterations = 1000;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);
    let resp = interp
        .execute(
            r"
x = 0
while True:
    x += 1
    if x >= 10:
        break
print(x)
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
}

// --- Recursion-depth cap ---

/// Unbounded user-function recursion must surface as `RecursionLimitExceeded`
/// rather than bleeding the memory budget. Prior to #401 the interpreter had
/// no frame-depth cap; a `def f(): f()` exhausted memory and reported
/// `LimitExceeded(memory…)`. Uses a small cap (10) in tests: the harness runs
/// on a 2 MB-stack thread, and debug-mode async state machines inflate per
/// frame. The production default (1000) is configured via `InterpreterConfig`.
#[tokio::test]
async fn resource_limits_recursion_unbounded_self_call_errors() {
    let mut cfg = InterpreterConfig::default();
    cfg.max_recursion_depth = 5;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);
    let resp = interp
        .execute(
            r"
def f():
    return f()

f()
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    let err = resp.error.expect("infinite recursion must surface an error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("RecursionLimitExceeded") || msg.contains("maximum recursion depth"),
        "expected RecursionLimitExceeded, got: {msg}"
    );
}

/// Recursion that stays under the cap must succeed — confirms the counter is
/// decremented on exit and does not leak across calls.
#[tokio::test]
async fn resource_limits_recursion_under_cap_resets_counter() {
    let mut cfg = InterpreterConfig::default();
    cfg.max_recursion_depth = 30;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);
    let resp = interp
        .execute(
            r"
def count(n):
    if n == 0:
        return 0
    return 1 + count(n - 1)

# Shallow depth: native stack per frame is large; stay well under both
# the interpreter cap and the host stack ceiling.
a = count(8)
b = count(8)
print(a, b)
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
}

/// Recursion via lambda must also be bounded.
#[tokio::test]
async fn resource_limits_recursion_applies_to_lambdas() {
    let mut cfg = InterpreterConfig::default();
    // Set the cap just below the native-stack ceiling for the lambda
    // recursion path. Bumping requires further per-frame future-size
    // reduction in `call_lambda`; the def-time default-evaluation
    // landing enlarged `FunctionParams` slightly and trimmed the
    // headroom back from 8 to 6.
    cfg.max_recursion_depth = 8;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);
    let resp = interp
        .execute(
            r"
f = lambda g, n: 0 if n == 0 else g(g, n - 1)
f(f, 30)
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    let err = resp.error.expect("lambda recursion past limit must error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("RecursionLimitExceeded") || msg.contains("maximum recursion depth"),
        "expected RecursionLimitExceeded, got: {msg}"
    );
}

// --- Wall-clock execution timeout ---

#[tokio::test]
async fn resource_limits_terminate_via_wallclock_timeout() {
    let interp = {
        let mut cfg = InterpreterConfig::default();
        cfg.max_execution_time = Some(Duration::from_millis(50));
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg)
    };

    let resp = interp
        .execute(
            r"
x = 0
while True:
    x += 1
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(!resp.is_ok());
    let err = format!("{:?}", resp.error.unwrap());
    assert!(err.contains("time") || err.contains("execution"), "error should mention time: {err}");
}

// --- Concurrent-tool semaphore cap ---
//
// The `test_stress.rs` original ran two semaphore tests (cap=5 and cap=2);
// they exercise the same code path with different bounds. Keep the tighter
// case as the canonical assertion — a cap=2 violation is the more sensitive
// detector of a stale-counter bug.

#[tokio::test]
async fn resource_limits_semaphore_caps_concurrent_tool_calls() {
    let active = Arc::new(Mutex::new(0u32));
    let peak = Arc::new(Mutex::new(0u32));
    let total = Arc::new(Mutex::new(0u32));

    let mut tools = Tools::new();
    tools.insert(
        "track",
        ToolDefinition {
            handler: Arc::new(ConcurrencyCountingTool {
                active: active.clone(),
                peak: peak.clone(),
                total: total.clone(),
            }),
            parallelizable: true,
        },
    );

    let mut cfg = InterpreterConfig::default();
    cfg.max_operations = 500_000;
    cfg.max_while_iterations = 10_000;
    cfg.max_concurrent_tools = 2;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);

    let assignments: String = (0..15).fold(String::new(), |mut s, i| {
        let _ = writeln!(&mut s, "r{i} = track()");
        s
    });
    let uses: String = (0..15).map(|i| format!("r{i}")).collect::<Vec<_>>().join(" + ");
    let code = format!("{assignments}total = {uses}\nprint(total)");

    let resp = interp.execute(&code, &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    let peak_val = *peak.lock().unwrap();
    assert!(peak_val <= 2, "peak {peak_val} exceeded limit 2");
    assert_eq!(*total.lock().unwrap(), 15);
}

// --- Proxy-resolution stress (large fan-out, error propagation, falsy values, nested) ---

#[tokio::test]
async fn resource_limits_fan_out_20_concurrent_calls_preserve_order() {
    let count = Arc::new(Mutex::new(0u32));
    let mut tools = Tools::new();
    tools.insert(
        "count",
        ToolDefinition {
            handler: Arc::new(CountingTool { count: count.clone() }),
            parallelizable: true,
        },
    );

    let interp = stress_interpreter();
    let assignments: String = (0..20).fold(String::new(), |mut s, i| {
        let _ = writeln!(&mut s, "r{i} = count(i={i})");
        s
    });
    let checks: String = (0..20).map(|i| format!("r{i} == {i}")).collect::<Vec<_>>().join(" and ");
    let code = format!("{assignments}all_correct = {checks}\nprint(all_correct)");

    let resp = interp.execute(&code, &tools, HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "True");
    assert_eq!(*count.lock().unwrap(), 20);
}

#[tokio::test]
async fn resource_limits_single_failure_among_many_propagates() {
    let mut tools = Tools::new();
    tools.insert(
        "maybe_fail",
        ToolDefinition { handler: Arc::new(MaybeFailTool), parallelizable: true },
    );

    let interp = stress_interpreter();
    let code = r#"
results = []
for i in range(10):
    results.append(maybe_fail(fail=(i == 7)))
output = []
for r in results:
    output.append(str(r))
print(",".join(output))
"#;
    let resp = interp.execute(code, &tools, HashMap::new()).await;
    assert!(resp.error.is_some(), "should have errored on index 7");
}

#[tokio::test]
async fn resource_limits_error_does_not_corrupt_subsequent_execution() {
    let mut tools = Tools::new();

    struct FailTool;
    #[async_trait]
    impl ToolHandler for FailTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Err(ToolError::new("boom"))
        }
    }
    struct WorkTool;
    #[async_trait]
    impl ToolHandler for WorkTool {
        async fn call(&self, _kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            Ok(Value::String("works".into()))
        }
    }

    tools.insert("failing", ToolDefinition { handler: Arc::new(FailTool), parallelizable: true });
    tools.insert("working", ToolDefinition { handler: Arc::new(WorkTool), parallelizable: true });

    let interp = stress_interpreter();

    let resp1 = interp.execute("result = failing()\nprint(result)", &tools, HashMap::new()).await;
    assert!(resp1.error.is_some());

    let resp2 = interp.execute("result = working()\nprint(result)", &tools, HashMap::new()).await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert_eq!(resp2.stdout.trim(), "works");
}

#[tokio::test]
async fn resource_limits_proxy_resolves_false_as_falsy() {
    let mut tools = Tools::new();
    tools.insert(
        "get_false",
        ToolDefinition { handler: Arc::new(BoolTool { value: false }), parallelizable: true },
    );

    let interp = stress_interpreter();
    let resp = interp
        .execute(
            "val = get_false()\nif val:\n    print('truthy')\nelse:\n    print('falsy')",
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "falsy");
}

#[tokio::test]
async fn resource_limits_proxy_resolves_zero_as_falsy() {
    let mut tools = Tools::new();
    tools.insert(
        "get_zero",
        ToolDefinition { handler: Arc::new(IntTool { value: 0 }), parallelizable: true },
    );

    let interp = stress_interpreter();
    let resp = interp
        .execute(
            "val = get_zero()\nif val:\n    print('truthy')\nelse:\n    print('falsy')",
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "falsy");
}

#[tokio::test]
async fn resource_limits_proxy_resolves_empty_string_as_falsy() {
    let mut tools = Tools::new();
    tools.insert(
        "get_empty",
        ToolDefinition { handler: Arc::new(EmptyStringTool), parallelizable: true },
    );

    let interp = stress_interpreter();
    let resp = interp
        .execute(
            "val = get_empty()\nif val:\n    print('truthy')\nelse:\n    print('falsy')",
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "falsy");
}

#[tokio::test]
async fn resource_limits_proxy_inside_dict_values() {
    let mut tools = Tools::new();
    tools.insert("fetch", ToolDefinition { handler: Arc::new(FetchTool), parallelizable: true });

    let interp = stress_interpreter();
    let resp = interp
        .execute(
            r#"
data = {}
keys = ["alpha", "beta", "gamma"]
for k in keys:
    data[k] = fetch(v=k)
parts = []
for k in keys:
    parts.append(data[k])
print(",".join(parts))
"#,
            &tools,
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "alpha,beta,gamma");
}

#[tokio::test]
async fn resource_limits_repeated_execution_does_not_leak_proxies() {
    let mut tools = Tools::new();

    struct EchoTool;
    #[async_trait]
    impl ToolHandler for EchoTool {
        async fn call(&self, kwargs: HashMap<String, Value>) -> Result<Value, ToolError> {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(kwargs.get("msg").cloned().unwrap_or(Value::String("".into())))
        }
    }

    tools.insert("echo", ToolDefinition { handler: Arc::new(EchoTool), parallelizable: true });

    let interp = stress_interpreter();
    for i in 0..5 {
        let code = format!("result = echo(msg='iteration_{i}')\nprint(result)");
        let resp = interp.execute(&code, &tools, HashMap::new()).await;
        assert!(resp.error.is_none(), "failed on iteration {i}: {:?}", resp.error);
        assert_eq!(resp.stdout.trim(), format!("iteration_{i}"));
    }
}

// --- AST-depth DoS guard ---

#[tokio::test]
async fn dos_deep_parens_clean_error() {
    // A paren bomb must fail cleanly (SyntaxError/limit), never SIGSEGV the host.
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let n = 100_000;
    let code = format!("x = {}1{}", "(".repeat(n), ")".repeat(n));
    let resp = interp.execute(&code, &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected clean error for deep-paren input");
}

#[tokio::test]
async fn dos_deep_chain_clean_error() {
    // A long left-associative chain (no brackets) yields a deep AST: the sync
    // numeric fast path recurses per operator and its Drop is deep. Must be a
    // clean RecursionError, never a SIGABRT. ~200 KB, under the source cap.
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let n = 100_000;
    let code = format!("x = 1{}\nprint(x)", "+1".repeat(n));
    let resp = interp.execute(&code, &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected clean recursion error for deep chain");
}

#[tokio::test]
async fn dos_oversized_source_rejected() {
    // Above the byte cap: clean SyntaxError, never a crash.
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let code = format!("x = '{}'", "a".repeat(600 * 1024));
    let resp = interp.execute(&code, &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected oversized source to be rejected");
}

#[tokio::test]
async fn dos_large_shallow_program_ok() {
    // Many shallow statements (large but not deep) must still run.
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let mut code = String::new();
    for i in 0..5000 {
        code.push_str(&format!("v{i} = {i}\n"));
    }
    code.push_str("print(v4999)\n");
    let resp = interp.execute(&code, &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "4999");
}

#[tokio::test]
async fn dos_normal_nesting_still_ok() {
    // Reasonable nesting well under the limit must still evaluate.
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let code = "x = ((((1 + 2)) * 3))\nprint(x)";
    let resp = interp.execute(code, &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "9");
}

#[tokio::test]
async fn dos_deep_list_clean_error() {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let n = 100_000;
    let code = format!("x = {}1{}", "[".repeat(n), "]".repeat(n));
    let resp = interp.execute(&code, &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected clean error for deep nested list");
}

#[tokio::test]
async fn dos_deep_call_clean_error() {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let n = 100_000;
    let code = format!("x = {}1{}", "abs(".repeat(n), ")".repeat(n));
    let resp = interp.execute(&code, &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected clean error for deep nested calls");
}

#[tokio::test]
async fn dos_deep_attribute_chain_clean_error() {
    // a.b.c.d… (no brackets) recurses the async attribute path — must raise
    // RecursionError, not overflow / grow the stack unbounded.
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let code = format!("v = x{}", ".a".repeat(50_000));
    let resp = interp.execute(&code, &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected clean error for deep attribute chain");
}

#[tokio::test]
async fn dos_deep_not_chain_clean_error() {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let code = format!("v = {}True", "not ".repeat(50_000));
    let resp = interp.execute(&code, &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected clean error for deep not chain");
}

#[tokio::test]
async fn dos_deep_str_concat_chain_clean_error() {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let code = format!("v = 'a'{}", "+'a'".repeat(50_000));
    let resp = interp.execute(&code, &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected clean error for deep str-concat chain");
}

#[tokio::test]
async fn dos_shallow_attribute_and_ops_ok() {
    // Ordinary nesting/attribute access is well under the limit and must work.
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let code = "class C:\n    def __init__(self): self.v = 5\no = C()\nprint(o.v + 1 + 2 + 3)\nprint('a' + 'b' + 'c')\nprint(not not True)\n";
    let resp = interp.execute(code, &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "11\nabc\nTrue");
}

#[tokio::test]
async fn dos_deep_subscript_chain_clean_error() {
    // a[0][0][0]… — sequential subscripts don't increase bracket depth, so the
    // parse-time bracket guard misses them; the eval_place depth guard catches
    // them instead.
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let code = format!("v = a{}", "[0]".repeat(50_000));
    let resp = interp.execute(&code, &Tools::new(), HashMap::new()).await;
    assert!(resp.error.is_some(), "expected clean error for deep subscript chain");
}

#[tokio::test]
async fn dos_deep_nested_statements_clean_error() {
    // Nested compound statements with 1-space-per-level indentation keep the
    // source small (~O(depth^2)/2) so ~700 levels fit under the byte cap and
    // exercise the statement-eval recursion. Must not overflow the host stack.
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let mut code = String::new();
    for i in 0..700 {
        for _ in 0..i {
            code.push(' ');
        }
        code.push_str("if True:\n");
    }
    for _ in 0..700 {
        code.push(' ');
    }
    code.push_str("x = 1\n");
    assert!(code.len() < 512 * 1024, "test source should fit under the cap: {}", code.len());
    let resp = interp.execute(&code, &Tools::new(), HashMap::new()).await;
    let _ = resp.error; // bounded outcome, never a crash
}

// --- Materialisation / allocation caps: these must fail closed with a clean
// error, never allocate their way to an uncatchable process abort (SIGABRT). ---

#[tokio::test]
async fn resource_limits_list_of_huge_range_is_rejected_not_oom() {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let resp = interp.execute("x = list(range(10**12))", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "materialising a 10**12 range must error, not OOM");
    let msg = format!("{:?}", resp.error.unwrap());
    assert!(msg.contains("too large") || msg.contains("limit"), "unexpected error: {msg}");
}

#[tokio::test]
async fn resource_limits_bytes_of_huge_count_is_rejected_not_oom() {
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let resp = interp.execute("x = bytes(10**12)", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "bytes(10**12) must error, not allocate a terabyte");
    let msg = format!("{:?}", resp.error.unwrap());
    assert!(msg.contains("maximum size") || msg.contains("limit"), "unexpected error: {msg}");
}

#[tokio::test]
async fn resource_limits_giant_integer_power_is_rejected_before_computing() {
    // (2**1000000)**1000000 would be a ~10**12-bit (~130 GB) integer; the
    // predicted-bit-length check must reject it before `pow` builds it.
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let resp = interp.execute("x = (2**1000000) ** 1000000", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "an over-limit integer power must error, not OOM");
    let msg = format!("{:?}", resp.error.unwrap());
    assert!(msg.contains("too large") || msg.contains("Overflow"), "unexpected error: {msg}");
}

#[tokio::test]
async fn resource_limits_legal_large_power_still_computes() {
    // The predictive reject must not be over-eager: 2**1000000 fits the default
    // max_int_bits (1_048_576) and must still succeed.
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    let resp = interp.execute("x = 2 ** 1000000\nprint(x > 0)", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "2**1000000 is legal: {:?}", resp.error);
    assert_eq!(resp.stdout, "True\n");
}

#[tokio::test]
async fn resource_limits_deeply_nested_value_build_does_not_overflow_stack() {
    // Loop-built deep nesting never trips the recursion limit, yet every walk
    // over the value (memory sizing on each assignment, drop at teardown) must
    // grow the stack rather than overflow it and abort the process.
    let interp =
        Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default());
    // Depth 50_000: far past the ~1000-frame native-stack overflow the stacker
    // guard prevents, and — now that per-assignment memory sizing reads the
    // container's O(1) cached size instead of re-walking — this completes in
    // O(n), not the O(n^2) that made even 5_000 time out.
    let resp = interp
        .execute(
            "a = []\nfor _ in range(50000):\n    a = [a]\nprint('built')",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    // The assertion is simply that we returned at all (no SIGSEGV/SIGABRT).
    assert!(resp.error.is_none(), "deep nesting should build cleanly: {:?}", resp.error);
    assert_eq!(resp.stdout, "built\n");
}
