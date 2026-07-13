---
id: migrate-parser-to-ruff-python-parser-drop-lgpl
title: "Migrate rustpython-parser -> ruff_python_parser (drops LGPL + 7 advisories)"
status: ready
priority: p2
dependencies: []
related: []
scopes: [core, eval, meta]
shared_scopes: []
paths: [crates/interpretthis/src/parser.rs, crates/interpretthis/src/eval/**, crates/interpretthis/Cargo.toml, Cargo.toml, deny.toml, NOTICE]
tags: [licensing, supply-chain, parser, dependencies]
---
Swap the Python parser from `rustpython-parser` 0.4 to `ruff_python_parser` (Astral, MIT).

**Why this became load-bearing.** `rustpython-parser` -> `rustpython-ast` -> `malachite-bigint` -> **`malachite`, LGPL-3.0-only**. RustPython's AST stores integer literals as malachite bignums, so malachite is statically linked into every binary we build even if no large integer is ever evaluated. Nothing we wrote asked for it — we already depend on `num-bigint` (MIT/Apache) directly for `Value::BigInt`.

This was inert while crates.io was the only registry: shipping *source* leaves the LGPL obligation with whoever compiles it. It stopped being inert the moment we started publishing **binary** PyPI wheels and npm `.node` addons — we are now the distributor of a work that statically links LGPL-3.0 code, which attaches LGPL-3.0 §4 relink obligations to every artifact (documented in `NOTICE`, which ships inside both packages). It also propagates: a downstream user statically linking `interpretthis` into a closed-source product inherits the same obligation, and "LGPL + static linking" is a common auto-flag in corporate OSS intake.

**The second payoff.** `deny.toml` currently ignores **seven** RUSTSEC advisories, and every one of them is `unic-*` or `paste` reached through `rustpython-parser` 0.4, each with the note "no safe upgrade". They all disappear with the parser.

**Scope.** This is a real migration, not a dependency bump: `crates/interpretthis/src/parser.rs` plus every AST match arm across `src/eval/**`. The two ASTs differ in shape (ruff's is arena/range-based and its node types are not the same enums), so the eval layer's pattern matches need rewriting rather than renaming.

**Done when:**
- `cargo tree -i malachite-base` returns nothing.
- `deny.toml` has an empty `advisories.ignore` list, and `LGPL-3.0-only` is gone from the licence allowlist.
- `NOTICE` no longer needs the third-party binary section.
- The full parity corpus and all 717 tests still pass — the corpus is the safety net that makes this migration checkable rather than hopeful.
