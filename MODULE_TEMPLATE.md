# Adding a stdlib module

Maintainer walkthrough for shipping a new allowlisted stdlib module in
`interpretthis` (invited contributors only — see [`CONTRIBUTING.md`](./CONTRIBUTING.md)).
The running example is a flat module named `urllib_parse` (import name
`urllib_parse` — **not** a dotted `urllib.parse` package; dotted imports are
rejected by the interpreter).

Also read [`CONFORMANCE.md`](./CONFORMANCE.md) if your module diverges from
CPython, and [`THREAT_MODEL.md`](./THREAT_MODEL.md) if it expands the trust
boundary.

## What you change

1. Create `src/eval/modules/<name>.rs` with `impl Module for XModule`.
2. Add `pub mod <name>;` in `src/eval/modules/mod.rs`.
3. Register `&name::XModule` in the `MODULES` array (same file). The array
   length must match the number of entries.
4. Add differential snippets under
   `tests/integration/parity_corpus/modules/<name>/*.py`.

The `MODULES` registry **is** the import allowlist. There is no second list.

Do **not** add new names to `AUTO_IMPORTED` (`json` / `re` / `datetime` are a
historical carve-out and closed).

## Module skeleton

Follow an existing small module (e.g. `src/eval/modules/textwrap.rs` or
`hashlib.rs`) for the full `async_trait` + `Module` impl. Minimal shape:

```rust
// src/eval/modules/urllib_parse.rs

use async_trait::async_trait;
use indexmap::IndexMap;

use super::Module;
use crate::{
    error::{EvalResult, InterpreterError},
    state::InterpreterState,
    tools::Tools,
    value::Value,
};

pub struct UrllibParseModule;

#[async_trait]
impl Module for UrllibParseModule {
    fn name(&self) -> &'static str {
        "urllib_parse"
    }

    fn has_function(&self, name: &str) -> bool {
        matches!(name, "quote" | "unquote" /* … */)
    }

    async fn call(
        &self,
        _state: &mut InterpreterState,
        func: &str,
        args: &[Value],
        _kwargs: &IndexMap<String, Value>,
        _tools: &Tools,
    ) -> EvalResult {
        match func {
            "quote" => quote_impl(args),
            // …
            other => Err(InterpreterError::AttributeError(format!(
                "module 'urllib_parse' has no attribute '{other}'"
            ))
            .into()),
        }
    }
}
```

Prefer `pub(crate)` for helpers so they are not mistaken for public surface.

## Registry edit

```rust
pub mod urllib_parse;

static MODULES: LazyLock<HashMap<&'static str, &'static dyn Module>> = LazyLock::new(|| {
    let modules: [&'static dyn Module; N] = [
        // …existing…
        &urllib_parse::UrllibParseModule,
    ];
    modules.into_iter().map(|m| (m.name(), m)).collect()
});
```

After registration:

```python
import urllib_parse
print(urllib_parse.quote("a b"))
```

`import urllib.parse` / `urllib.parse.quote` will **not** work (no dotted
imports).

## Parity corpus

Put one focused `.py` file per behaviour under
`tests/integration/parity_corpus/modules/<name>/`. The runner byte-diffs
stdout against host `python3.12`.

Hygiene:

- No clock, network, or filesystem.
- Print sets/dicts via `sorted(...)` when order would otherwise differ.
- Prefer under ~30 lines; filename is the test name (`quote_basic.py`).

## Conformance links

If the module intentionally diverges, add a section + stable anchor in
`CONFORMANCE.md` and end user-facing `"…not supported"` errors with
`(see CONFORMANCE.md#<anchor>)`.

## Verify

```bash
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
# filter parity for your module when nextest is available:
# cargo nextest run parity::modules::urllib_parse
```

## PR checklist

1. Module registered in `MODULES` with correct array length.
2. Corpus covers every public function.
3. `CONFORMANCE.md` updated only if behaviour diverges (state “no” if not).
4. No new auto-imports; no dotted module names.
