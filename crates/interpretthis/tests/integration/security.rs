// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Phase 4: Security tests.
//! Ported from Python `test_dangerous_builtins.py`, `test_sandbox_escape_security.py`,
//! and `test_lazy_proxy_security.py`.
//!
//! These validate that dangerous operations are blocked and the sandbox
//! cannot be escaped via introspection chains.

use std::collections::HashMap;

use interpretthis::{Interpreter, InterpreterConfig, InterpreterDeps, Tools};

fn interpreter() -> Interpreter {
    Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default())
}

fn no_tools() -> Tools {
    Tools::new()
}

// --- Dangerous builtins blocked ---

#[tokio::test]
async fn security_getattr_blocked_dunder() {
    // Bounded getattr allows safe names but still rejects class-walk dunders.
    let interp = interpreter();
    let resp = interp.execute("x = getattr([], '__class__')", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
    let msg = resp.error.unwrap().to_string();
    assert!(
        msg.contains("__class__") || msg.contains("Security") || msg.contains("not permitted"),
        "expected security block, got: {msg}"
    );
}

#[tokio::test]
async fn security_class_attribute_access_blocked() {
    // Direct attribute access still blocks class-walk dunders.
    let interp = interpreter();
    let resp = interp.execute("x = ().__class__", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_setattr_blocked_dunder() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
class C:
    pass
o = C()
setattr(o, '__class__', 1)
"#,
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_delattr_blocked_dunder() {
    let interp = interpreter();
    let resp = interp.execute("delattr([], '__class__')", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_getattr_safe_three_arg() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
class C:
    pass
o = C()
print(getattr(o, 'missing', 42))
setattr(o, 'x', 7)
print(getattr(o, 'x'))
delattr(o, 'x')
print(getattr(o, 'x', 'gone'))
"#,
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "{:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "42\n7\ngone");
}

#[tokio::test]
async fn security_vars_blocked() {
    let interp = interpreter();
    let resp = interp.execute("x = vars()", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_dir_blocked() {
    let interp = interpreter();
    let resp = interp.execute("x = dir([])", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_eval_blocked() {
    let interp = interpreter();
    let resp = interp.execute("eval('1+1')", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_exec_blocked() {
    let interp = interpreter();
    let resp = interp.execute("exec('x = 1')", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_compile_blocked() {
    let interp = interpreter();
    let resp =
        interp.execute("compile('x = 1', '<string>', 'exec')", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_dunder_import_blocked() {
    let interp = interpreter();
    let resp = interp.execute("__import__('os')", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

// --- Safe builtins still available ---

#[tokio::test]
async fn security_hasattr_available() {
    let interp = interpreter();
    let resp =
        interp.execute("x = hasattr([], 'append')\nprint(x)", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "True");
}

#[tokio::test]
async fn security_type_available() {
    let interp = interpreter();
    // `type(x)` yields a type object; printing it matches CPython's
    // `<class 'int'>`, and `.__name__` gives the bare name.
    let resp = interp.execute("x = type(42)\nprint(x)", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "<class 'int'>");

    let named = interp.execute("print(type(42).__name__)", &no_tools(), HashMap::new()).await;
    assert!(named.error.is_none(), "error: {:?}", named.error);
    assert_eq!(named.stdout.trim(), "int");
}

#[tokio::test]
async fn security_isinstance_available() {
    let interp = interpreter();
    let resp =
        interp.execute("x = isinstance(42, int)\nprint(x)", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "True");
}

// --- Name protection ---

#[tokio::test]
async fn security_cannot_define_function_with_dangerous_name() {
    let interp = interpreter();
    let resp = interp.execute("def eval(x):\n    return x", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_dangerous_name_in_assignment() {
    let interp = interpreter();
    let resp = interp.execute("eval = 42", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

// --- Attribute access blocking ---

#[tokio::test]
async fn security_dunder_globals_blocked() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r"
def f():
    pass
x = f.__globals__
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_dunder_code_blocked() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r"
def f():
    pass
x = f.__code__
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_dunder_subclasses_blocked() {
    let interp = interpreter();
    let resp = interp.execute("x = int.__subclasses__()", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_dunder_bases_blocked() {
    let interp = interpreter();
    let resp = interp.execute("x = int.__bases__", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_dunder_dict_blocked() {
    let interp = interpreter();
    let resp = interp.execute("x = (42).__dict__", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

// --- Import statements blocked ---

#[tokio::test]
async fn security_import_statement_blocked() {
    let interp = interpreter();
    let resp = interp.execute("import os", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_from_import_blocked() {
    let interp = interpreter();
    let resp = interp.execute("from os import path", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

// --- File operations blocked ---

#[tokio::test]
async fn security_open_blocked() {
    let interp = interpreter();
    let resp = interp.execute("f = open('/etc/passwd')", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

// --- DoS prevention: unbounded allocation ---

#[tokio::test]
async fn security_list_multiplication_limit() {
    let interp = interpreter();
    let resp = interp.execute("x = [0] * 100000000", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "should reject huge list multiplication");
}

#[tokio::test]
async fn security_string_multiplication_limit() {
    let interp = interpreter();
    let resp = interp.execute("x = 'a' * 200000000", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "should reject huge string multiplication");
}

#[tokio::test]
async fn security_format_width_limit() {
    let interp = interpreter();
    let resp = interp.execute("x = f'{1:>100000}'", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "should reject huge format width");
}

#[tokio::test]
async fn security_integer_overflow_detected() {
    // Arbitrary-precision ints: max_i64+1 is valid. Cap absurd powers instead.
    let interp = interpreter();
    let resp = interp.execute("x = 2 ** 2000000", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "should reject absurdly large integer power");
}

#[tokio::test]
async fn security_input_blocked() {
    let interp = interpreter();
    let resp = interp.execute("x = input()", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "input() should be blocked");
}

// --- Memory budget enforcement ---

#[tokio::test]
async fn security_memory_limit_large_list() {
    let mut cfg = InterpreterConfig::default();
    cfg.max_memory_bytes = 1024;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);
    let resp = interp
        .execute(
            r"
x = []
for i in range(10000):
    x.append(i)
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_some(), "should hit memory limit");
    let err = format!("{:?}", resp.error.unwrap());
    assert!(
        err.contains("memory") || err.contains("limit") || err.contains("Limit"),
        "error should mention memory: {err}"
    );
}

#[tokio::test]
async fn security_memory_limit_large_string() {
    let mut cfg = InterpreterConfig::default();
    cfg.max_memory_bytes = 1024;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);
    let resp = interp.execute("x = 'a' * 5000", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some(), "should hit memory limit");
}

#[tokio::test]
async fn security_memory_limit_large_dict() {
    let mut cfg = InterpreterConfig::default();
    cfg.max_memory_bytes = 1024;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);
    let resp = interp
        .execute(
            r"
d = {}
for i in range(10000):
    d[str(i)] = i
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_some(), "should hit memory limit");
}

#[tokio::test]
async fn security_memory_limit_string_concat_loop() {
    let mut cfg = InterpreterConfig::default();
    cfg.max_memory_bytes = 2048;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);
    let resp = interp
        .execute(
            r#"
s = ""
for i in range(10000):
    s = s + "aaaa"
"#,
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_some(), "should hit memory limit");
}

#[tokio::test]
async fn security_memory_within_limit_ok() {
    let mut cfg = InterpreterConfig::default();
    cfg.max_memory_bytes = 10 * 1024 * 1024;
    let interp = Interpreter::new(InterpreterDeps { tools: Tools::new() }, cfg);
    let resp = interp
        .execute("x = [i for i in range(100)]\nprint(len(x))", &no_tools(), HashMap::new())
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "100");
}

// --- Small multiplications still work ---

#[tokio::test]
async fn security_small_list_multiplication_ok() {
    let interp = interpreter();
    let resp = interp.execute("x = [0] * 100\nprint(len(x))", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "100");
}

#[tokio::test]
async fn security_small_string_multiplication_ok() {
    let interp = interpreter();
    let resp = interp.execute("x = 'ab' * 50\nprint(len(x))", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "100");
}

#[tokio::test]
async fn security_small_format_width_ok() {
    let interp = interpreter();
    #[expect(
        clippy::literal_string_with_formatting_args,
        reason = "Python f-string literal fed to the interpreter, not Rust format"
    )]
    let src = "x = f'{42:>10}'\nprint(x)";
    let resp = interp.execute(src, &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim_end(), "        42");
}

// --- vars(): bounded instance-only builtin ---

#[tokio::test]
async fn security_vars_instance_ok() {
    // The supported form: vars(instance) exposes only its fields (a copy),
    // which are all already reachable via getattr.
    let interp = interpreter();
    let resp = interp
        .execute(
            r"
class C:
    def __init__(self):
        self.a = 1
        self.b = 2
print(vars(C()) == {'a': 1, 'b': 2})
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "True");
}

#[tokio::test]
async fn security_vars_no_arg_blocked() {
    // vars() with no args == locals(), which the sandbox does not expose.
    let interp = interpreter();
    let resp = interp.execute("x = vars()", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_vars_module_rejected() {
    // vars(module) would re-expose module internals — rejected (narrower than CPython).
    let interp = interpreter();
    let resp = interp.execute("import math\nx = vars(math)", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn security_vars_non_instance_rejected() {
    let interp = interpreter();
    for src in ["x = vars(1)", "x = vars('s')", "x = vars([1])", "x = vars({})"] {
        let resp = interp.execute(src, &no_tools(), HashMap::new()).await;
        assert!(resp.error.is_some(), "expected vars() to reject: {src}");
    }
}

#[tokio::test]
async fn security_locals_globals_still_blocked() {
    // Removing `vars` from the denylist must not have loosened locals/globals.
    let interp = interpreter();
    for src in ["x = locals()", "x = globals()"] {
        let resp = interp.execute(src, &no_tools(), HashMap::new()).await;
        assert!(resp.error.is_some(), "expected block for: {src}");
    }
}
