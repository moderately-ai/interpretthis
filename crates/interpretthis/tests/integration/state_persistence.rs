// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Phase 5: State persistence tests.
//! Ported from Python `test_interpreter.py`, `test_asyncio_pickling.py`,
//! and `test_pickling`_*.py.
//!
//! These validate that interpreter state can be exported and imported
//! across interpreter instances, preserving variables and functions.

use std::collections::HashMap;

use interpretthis::{Interpreter, InterpreterConfig, InterpreterDeps, Tools};

fn interpreter() -> Interpreter {
    Interpreter::new(InterpreterDeps { tools: Tools::new() }, InterpreterConfig::default())
}

fn no_tools() -> Tools {
    Tools::new()
}

#[tokio::test]
async fn state_basic_round_trip_preserves_variables() {
    let interp = interpreter();
    let resp = interp.execute("x = 42\ny = 'hello'", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    // Export
    let state_bytes = interp.export_state().expect("export failed");

    // Import into fresh interpreter
    let interp2 = interpreter();
    interp2.import_state(&state_bytes).expect("import failed");

    let resp2 = interp2.execute("print(f'{x},{y}')", &no_tools(), HashMap::new()).await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert_eq!(resp2.stdout.trim(), "42,hello");
}

#[tokio::test]
async fn state_function_persistence() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
def greet(name):
    return "Hello, " + name
"#,
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let state_bytes = interp.export_state().expect("export failed");
    let interp2 = interpreter();
    interp2.import_state(&state_bytes).expect("import failed");

    let resp2 = interp2.execute("print(greet('World'))", &no_tools(), HashMap::new()).await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert_eq!(resp2.stdout.trim(), "Hello, World");
}

#[tokio::test]
async fn state_nested_function_persistence() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r"
def outer(x):
    def inner(y):
        return x + y
    return inner(10)
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let state_bytes = interp.export_state().expect("export failed");
    let interp2 = interpreter();
    interp2.import_state(&state_bytes).expect("import failed");

    let resp2 = interp2.execute("print(outer(5))", &no_tools(), HashMap::new()).await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert_eq!(resp2.stdout.trim(), "15");
}

#[tokio::test]
async fn state_recursive_function_persistence() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r"
def factorial(n):
    if n <= 1:
        return 1
    return n * factorial(n - 1)
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let state_bytes = interp.export_state().expect("export failed");
    let interp2 = interpreter();
    interp2.import_state(&state_bytes).expect("import failed");

    // Shallow depth: native stack per Python frame is large until the
    // eval trampoline lands (see AGENTS.md).
    let resp2 = interp2.execute("print(factorial(6))", &no_tools(), HashMap::new()).await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert_eq!(resp2.stdout.trim(), "720");
}

#[tokio::test]
async fn state_function_with_default_args_persistence() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
def greet(name, greeting="Hello"):
    return f"{greeting}, {name}!"
"#,
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let state_bytes = interp.export_state().expect("export failed");
    let interp2 = interpreter();
    interp2.import_state(&state_bytes).expect("import failed");

    let resp2 = interp2.execute("print(greet('World'))", &no_tools(), HashMap::new()).await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert_eq!(resp2.stdout.trim(), "Hello, World!");
}

#[tokio::test]
async fn state_function_with_kwargs_persistence() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r"
def make_dict(**kwargs):
    return str(kwargs)
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let state_bytes = interp.export_state().expect("export failed");
    let interp2 = interpreter();
    interp2.import_state(&state_bytes).expect("import failed");

    let resp2 = interp2.execute("print(make_dict(a=1, b=2))", &no_tools(), HashMap::new()).await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert!(!resp2.stdout.trim().is_empty());
}

#[tokio::test]
async fn state_closure_persistence() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r"
multiplier = 10
def scale(x):
    return x * multiplier
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let state_bytes = interp.export_state().expect("export failed");
    let interp2 = interpreter();
    interp2.import_state(&state_bytes).expect("import failed");

    let resp2 = interp2.execute("print(scale(5))", &no_tools(), HashMap::new()).await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert_eq!(resp2.stdout.trim(), "50");
}

#[tokio::test]
async fn state_keys_after_export_import() {
    let interp = interpreter();
    let resp = interp.execute("x = 1\ny = 2\n_internal = 3", &no_tools(), HashMap::new()).await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let state_bytes = interp.export_state().expect("export failed");
    let interp2 = interpreter();
    interp2.import_state(&state_bytes).expect("import failed");

    let keys = interp2.state_keys();
    assert!(keys.contains(&"x".to_string()));
    assert!(keys.contains(&"y".to_string()));
    // Internal keys (starting with _) should be filtered
    assert!(!keys.contains(&"_internal".to_string()));
}

#[tokio::test]
async fn state_data_structures_persistence() {
    let interp = interpreter();
    let resp = interp
        .execute(
            "my_list = [1, 2, 3]\nmy_dict = {'a': 1}\nmy_tuple = (4, 5)",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);

    let state_bytes = interp.export_state().expect("export failed");
    let interp2 = interpreter();
    interp2.import_state(&state_bytes).expect("import failed");

    let resp2 = interp2
        .execute("print(f'{my_list},{my_dict},{my_tuple}')", &no_tools(), HashMap::new())
        .await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert!(!resp2.stdout.trim().is_empty());
}

#[tokio::test]
async fn state_function_reuse_across_calls() {
    let interp = interpreter();

    // First call: define function
    let resp1 = interp
        .execute(
            r"
def add(a, b):
    return a + b
",
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp1.error.is_none(), "error: {:?}", resp1.error);

    // Second call: use function
    let resp2 = interp.execute("print(add(3, 4))", &no_tools(), HashMap::new()).await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert_eq!(resp2.stdout.trim(), "7");
}

#[tokio::test]
async fn state_bigint_and_exception_group_round_trip() {
    let interp = interpreter();
    let resp = interp
        .execute(
            r#"
big = 2 ** 100
eg = ExceptionGroup("g", [ValueError("a"), TypeError("b")])
print(big)
print(len(eg.exceptions))
"#,
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp.error.is_none(), "error: {:?}", resp.error);
    assert_eq!(resp.stdout.trim(), "1267650600228229401496703205376\n2");

    let state_bytes = interp.export_state().expect("export failed");
    let interp2 = interpreter();
    interp2.import_state(&state_bytes).expect("import failed");

    let resp2 = interp2
        .execute(
            r#"
print(big)
print(type(eg).__name__)
print(len(eg.exceptions))
print(type(eg.exceptions[0]).__name__)
"#,
            &no_tools(),
            HashMap::new(),
        )
        .await;
    assert!(resp2.error.is_none(), "error: {:?}", resp2.error);
    assert_eq!(
        resp2.stdout.trim(),
        "1267650600228229401496703205376\nExceptionGroup\n2\nValueError"
    );
}
