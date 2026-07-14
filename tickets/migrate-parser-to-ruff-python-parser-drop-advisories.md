---
id: migrate-parser-to-ruff-python-parser-drop-advisories
title: "Migrate rustpython-parser -> ruff_python_parser (clears 7 RUSTSEC advisories)"
status: ready
priority: p3
dependencies: []
related: []
scopes: [core, eval, meta]
shared_scopes: []
paths: [crates/interpretthis/src/parser.rs, crates/interpretthis/src/eval/**, crates/interpretthis/Cargo.toml, Cargo.toml, deny.toml]
tags: [supply-chain, parser, dependencies]
---
Swap the Python parser from `rustpython-parser` 0.4 to `ruff_python_parser` (Astral, MIT).

**Priority dropped from p2 to p3.** The original driver — the LGPL-3.0-only `malachite` bignum statically linked into every shipped binary — is **already resolved** without a parser migration: `rustpython-parser` is now built with its `num-bigint` feature instead of the default `malachite-bigint`, so the whole malachite subtree is gone from the tree (`cargo tree -i malachite-base` returns nothing). `deny.toml` no longer allows any copyleft licence, and `NOTICE` no longer needs a third-party-binary section.

**What remains.** `deny.toml` still ignores **seven** RUSTSEC advisories, all `unic-*` or `paste` reached through `rustpython-parser` 0.4's lexer (not through malachite), each noted "no safe upgrade". These are the only reason left to move parsers. They are advisories on unmaintained crates, not active vulnerabilities in our usage, so this is hygiene rather than urgency — hence p3.

**Scope.** A real migration, not a dependency bump: `crates/interpretthis/src/parser.rs` plus every AST match arm across `src/eval/**`. ruff's AST is arena/range-based with different node types, so the eval layer's pattern matches need rewriting rather than renaming.

**Done when:**
- `deny.toml` has an empty `advisories.ignore` list.
- The full parity corpus and all tests still pass — the corpus is the safety net that makes this migration checkable rather than hopeful.
