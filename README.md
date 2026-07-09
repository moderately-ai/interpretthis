# interpretthis

Sandboxed Python AST interpreter for Rust.

Runs untrusted or LLM-generated Python by evaluating a
[`rustpython-parser`](https://crates.io/crates/rustpython-parser) AST under
resource limits and an allowlisted language surface. It is **not** CPython
embedded via the C API: there is no filesystem, network, or process access
unless the host injects those as tools.

## Why

You need to execute model-written Python for structured work (transforms,
scoring, tool-orchestrated agents) without giving the model a real Python
process. `interpretthis` is the evaluator: host code owns tools, limits, and
what happens with the result.

## Quick start

```rust
use std::collections::HashMap;

use interpretthis::{
    Interpreter, InterpreterConfig, InterpreterDeps, KwargsExt, ToolDefinition, Tools, Value,
};

#[tokio::main]
async fn main() {
    let tools = Tools::new().with(
        "double",
        ToolDefinition::from_fn(|kwargs| async move {
            let n = kwargs.require_int("n")?;
            Ok(Value::Int(n * 2))
        }),
    );

    let interp = Interpreter::new(
        InterpreterDeps { tools },
        InterpreterConfig::default(),
    );

    // Per-call tools merge with registered tools (per-call wins on name clash).
    let resp = interp
        .execute(
            "result = double(n=x)\nprint(result)",
            &Tools::new(),
            HashMap::from([("x".to_string(), Value::Int(42))]),
        )
        .await;

    match resp.result() {
        Ok(()) => println!("{}", resp.stdout),
        Err(e) => eprintln!("{e}"),
    }
}
```

Requires Rust 1.85+ and Tokio.

## Limits (honest)

- **Language subset** — large, but not full CPython. See
  [`CONFORMANCE.md`](./CONFORMANCE.md).
- **Sandbox** — dangerous names/attrs and non-allowlisted imports are rejected.
  See [`THREAT_MODEL.md`](./THREAT_MODEL.md).
- **Tools are trusted** — a host tool with side effects extends the trust
  boundary. Tool failures become a generic Python `Exception` (catchable);
  uncaught they fail the host `execute` call.
- **State export** is a versioned byte blob (4-byte version + JSON) for
  host-owned resume; signing/encryption is the host’s job.

## Docs

| | |
| --- | --- |
| [docs.rs/interpretthis](https://docs.rs/interpretthis) | API reference (after crates.io publish) |
| [`CONFORMANCE.md`](./CONFORMANCE.md) | CPython divergences |
| [`STATUS.md`](./STATUS.md) | Parity track status |
| [`THREAT_MODEL.md`](./THREAT_MODEL.md) | Security boundary |
| [`CHANGELOG.md`](./CHANGELOG.md) | Release notes |
| [`CONTRIBUTING.md`](./CONTRIBUTING.md) | Contribution policy (outreach first) |
| [`RELEASING.md`](./RELEASING.md) | Tag and crates.io publish |
| [`MODULE_TEMPLATE.md`](./MODULE_TEMPLATE.md) | Adding a stdlib module |
| [`SECURITY.md`](./.github/SECURITY.md) | Vulnerability reporting |

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
scripts/license-headers.sh check
```

Parity tests compare against host `python3.12` when available
(`tests/integration/parity_corpus/`).

## License

Licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](./LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([`LICENSE-MIT`](./LICENSE-MIT) or
  http://opensource.org/licenses/MIT)

at your option.

### Contribution policy

We do **not** accept unsolicited outside contributions. If you are interested
in working on this project, email
[opensource@moderately.ai](mailto:opensource@moderately.ai) and explain why —
see [`CONTRIBUTING.md`](./CONTRIBUTING.md).

If a contribution is later accepted, unless you explicitly state otherwise it
is dual-licensed under MIT OR Apache-2.0 as above, without additional terms.
