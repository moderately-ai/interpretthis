# Threat Model — interpretthis

## Why this document

`interpretthis` executes LLM-generated Python in production pipelines. The threat model is *"what an adversarial or prompt-injected LLM might emit"* — not a generic Python sandbox. The crate's job is structured-output generation under tool injection, not arbitrary code execution.

This file enumerates concrete attacks and points to the code that blocks them so a reader can verify the security posture without reverse-engineering the implementation. For vulnerability disclosure, see [`.github/SECURITY.md`](./.github/SECURITY.md).

## What's blocked at parse / eval time

Every restriction sits in one of four categories. Audit posture differs per category: a parser-rejected construct can't even be modelled in the value graph, while an allowlist-gated construct admits the syntax but stops execution at a vetted boundary. When a new language feature is added, identifying the category up-front is what determines whether the existing blocklists still cover it.

### 1. Parser-rejected

Constructs the parser refuses outright; they never reach the evaluator.

Currently: **none above the rustpython-parser baseline**. We use stock `rustpython-parser`, so all Python 3 syntax parses. AST nodes the evaluator doesn't handle reach a catch-all branch (next category), not a parser error. This is deliberate: every restriction is checked in our code, so adding a parser-side check is fragile (a `rustpython-parser` upgrade can silently lift it) compared to checking in the evaluator where we own the boundary.

### 2. Evaluator-rejected with a clear error

AST nodes that parse, but the evaluator refuses to execute. Most return `InterpreterError::Security` or `InterpreterError::Runtime`; matrix multiplication is the lone `InterpreterError::TypeError`. Prefer `(see CONFORMANCE.md#<anchor>)` on user-facing rejections where an anchor exists.

| Construct | Source location |
|---|---|
| `await` (no coroutine machinery) | `crates/interpretthis/src/eval/mod.rs::eval_expr` `Expr::Await` arm |
| `async def`, `async for`, `async with` | `crates/interpretthis/src/eval/mod.rs::eval_stmt` catch-all |
| `class` keyword arguments (e.g. `metaclass=`) | `crates/interpretthis/src/eval/classes.rs::eval_class_def` keyword-args check |
| Computed class bases (e.g. `class Foo(f()):`) | `crates/interpretthis/src/eval/classes.rs::eval_class_def` base-expr check — only bare-name bases accepted |
| `from module import *` | `crates/interpretthis/src/eval/modules/mod.rs::eval_import_from` star-import branch |
| Relative imports (`from .module`) | `crates/interpretthis/src/eval/modules/mod.rs::eval_import_from` |
| Dotted imports (`import a.b`) | `crates/interpretthis/src/eval/modules/mod.rs::eval_import` |
| Augmented assignment to a slice target | `crates/interpretthis/src/eval/statements.rs::eval_aug_assign` |
| Matrix multiplication (`@`) | `crates/interpretthis/src/eval/operations.rs::apply_binop` MatMult arm |
| Complex `del` attribute / subscript target | `crates/interpretthis/src/eval/delete.rs::delete_target` |

Supported (not in this rejection table): multi-level classes + C3 MRO + `super()`, class decorators (`@property` / `@staticmethod` / `@classmethod` / `@dataclass`), `match` class patterns, eager `yield` / `yield from`, and user-class `with` / `__enter__` / `__exit__` (`crates/interpretthis/src/eval/control_flow.rs::eval_with`). Generator *iterator* protocol (`next` / `send` / `throw` / `close`) remains partial — see [`CONFORMANCE.md`](./CONFORMANCE.md).

### 3. Allowlist-gated

The construct is supported in principle, but execution checks against a vetted whitelist.

- **Imports** — `crates/interpretthis/src/eval/modules/mod.rs::MODULES` is the registry of every shippable stdlib module (including `copy`). The registry IS the allowlist; `is_known_module` reads from it. Any other module raises `ModuleNotFoundError`.
- **Bare-name resolution** — `DANGEROUS_NAMES` at `crates/interpretthis/src/security/names.rs` rejects `eval`, `exec`, `compile`, `__import__`, `globals`, `locals`, `dir`, `open`, `file`, `os`, `sys`, `subprocess`, `shutil` even though the parser accepts them as identifiers. Checked in `crates/interpretthis/src/security/validator.rs`. `getattr` / `setattr` / `delattr` / `vars` are **bounded builtins**, not blocked: they resolve, but validate the attribute name against `BLOCKED_ATTRIBUTES` (and `vars` is instance-only, returning a copy of fields already reachable via `getattr`).
- **Attribute access** — `BLOCKED_ATTRIBUTES` at `crates/interpretthis/src/security/names.rs` gates `__class__`, `__bases__`, `__subclasses__`, `__mro__`, `__globals__`, `__code__`, `__closure__`, `__dict__`, `__builtins__`, `__spec__`, `__loader__`. All are blocked on **write** (the authoritative funnel is `validate_attribute`, called by every mutation site). All are blocked on **read** too, EXCEPT `__class__`: it aliases `type(x)` (already reachable via the `type()` builtin, so it grants no new capability) and is resolved to the type object by `eval::names::resolve_object_attr` before the read-side validator. `__class__` stays in the list so *assigning* it (in-sandbox type confusion) is still rejected. Single-underscore names (`obj._field`) are allowed; only the explicit dunder list is gated.

### 4. Dynamically validated

Runtime caps that don't refuse a construct but bound its impact. Catalogued in the next section.

## What's bounded by resource limits (not blocked outright)

These are intentional DoS controls — the operations themselves are legal in the language subset, but the runtime cuts them off before they hurt.

- **Memory** — `max_memory_bytes`, default 128 MiB (accounted state size).
- **Total operation count** — `max_operations`, default 10 M.
- **While-loop iterations** — `max_while_iterations`, default 100 K.
- **Recursion depth** — `max_recursion_depth`, default 1000 (matches CPython). Enforced at `crates/interpretthis/src/state.rs::enter_call`.
- **Stdout** — `max_stdout_bytes`, default 64 KiB.
- **Wall-clock** — optional `max_execution_time`; checked cooperatively every 100 ops (does not pre-empt a blocked tool future).
- **Collection / string multiply caps** — fixed ceilings on list/string repetition size (`MAX_COLLECTION_SIZE` / `MAX_STRING_SIZE` in `crates/interpretthis/src/eval/operations.rs`), independent of the memory budget.
- **Integer overflow** — arithmetic uses `checked_*` ops where applicable; overflow surfaces as a typed error, not a panic or wrap. Very large integer exponents may take a float fast-path rather than counting ops.

Defaults live in `crates/interpretthis/src/config.rs`; `InterpreterConfig` lets callers tighten or loosen each independently.

## What's relied on (assumptions, not enforced)

- **The categorization in "What's blocked" is the security boundary.** Every new language feature must be classified (parser-rejected / evaluator-rejected / allowlist-gated / dynamically-validated) before it ships and the corresponding blocklists re-audited. Adding a feature without confirming the blocklists still cover the new escape surface is the bug class this section exists to prevent.
- **Tool implementations are trusted code.** Tool errors surface as `InterpreterError::Tool` and are *not* catchable by user-code `try`/`except`. Adding a tool with side effects implicitly extends the trust boundary. Hosts that inject tools own that boundary.
- **Integrity of resumable state is a host responsibility.** This crate serializes interpreter variables with a `STATE_FORMAT_VERSION` prefix and rejects version-mismatched or truncated blobs via `InterpreterError::StateFormatSuperseded` (the size cap `MAX_IMPORT_SIZE` and version gate *do* fail closed). It does **not** sign or encrypt blobs, and it trusts the *contents* wholesale: import re-parses and re-caches each function/lambda/method **source** for execution, and reconstitutes variable **values** without re-validation. Two consequences a host must account for when resuming an untrusted blob: (1) resume is equivalent to executing the blob's embedded Python under the sandbox — safe only in the sense that the same runtime gates (`validate_name`/`validate_attribute`) still apply at execution; and (2) because values are reconstituted *past* the name-resolution gate, a crafted blob can carry a post-resolution builtin sentinel (e.g. `dir`, or an `is_known_module`-gated `__import__`) that normal code could never bind. Neither reaches a host-escape primitive — the effect is bounded to in-sandbox introspection and the already-allowlisted module set — but it is why **hosts that persist state across an untrusted boundary must wrap exports (e.g. HMAC-SHA256) and reject on verification failure** rather than relying on the format gate alone.

## Status notes

`DANGEROUS_NAMES` and `BLOCKED_ATTRIBUTES` remain the security spine for every shipped language feature (classes, decorators, match, generators, `with`, stdlib modules). New surface must re-audit those lists before merge.

Open parity work (not security blockers by themselves) is tracked in tickets and summarized in [`STATUS.md`](./STATUS.md) / [`CONFORMANCE.md`](./CONFORMANCE.md). Current known gaps include while-loop generator suspension, lazy iterator storage, selected stdlib method parity, class-pattern keywords, and clean perf-baseline recapture. `contextlib.nullcontext` / `suppress` ship; `contextlib.contextmanager` remains tracked separately.

## Cross-reference: `CONFORMANCE.md`

This document is the security-side view of what is rejected at parse / eval time and why each rejection exists in attacker-model terms. The user-side catalogue of what the interpreter does or does not support — across both security and parity dimensions, with stable section anchors that every "not supported" error string in `crates/interpretthis/src/` points at — lives in `CONFORMANCE.md` next to this file. Distinction: this doc explains *why* things are blocked from a security standpoint; `CONFORMANCE.md` is the user-facing catalogue of *what* the interpreter does or does not support. A user reading an `InterpreterError` message follows the `(see CONFORMANCE.md#...)` pointer in the error string to the relevant anchor; a security reviewer auditing the sandbox boundary reads this file.

## Concrete attack → mitigation

| Attack pattern                                              | Where it fails                                                  |
|-------------------------------------------------------------|-----------------------------------------------------------------|
| `__import__('os').system('rm -rf /')`                       | `__import__` in `DANGEROUS_NAMES` (`security/names.rs`)         |
| `().__class__.__bases__[0].__subclasses__()`                | `().__class__` resolves (to `tuple`), but `__bases__` / `__mro__` / `__subclasses__` stay blocked, so the walk dead-ends (`security/names.rs`) |
| `obj.__class__ = SomethingEvil` (type confusion)            | `__class__` write-blocked at `set_attr` / `setattr` / `__setattr__` (`validate_attribute`) |
| `eval("...")` / `exec("...")` / `compile(...)`              | `eval` / `exec` / `compile` in `DANGEROUS_NAMES`                |
| `getattr(obj, '__globals__')`                               | `getattr` validates the name against `BLOCKED_ATTRIBUTES`       |
| `setattr(obj, '__class__', 1)` / `delattr(obj, '__mro__')`  | `setattr` / `delattr` validate the name against `BLOCKED_ATTRIBUTES` (both are write ops; `__class__` is write-blocked even though its read is allowed) |
| `vars(fn)` / `vars(cls)` / `vars()`                         | `vars` is instance-only; module/class/no-arg forms raise `TypeError` |
| `import os` / `from os import path`                         | `os` not in the `MODULES` registry (`eval/modules/mod.rs`)      |
| `from . import sibling`                                     | relative-import gate in `eval/modules/mod.rs::eval_import_from` |
| `from collections import *`                                 | star-import gate in `eval/modules/mod.rs::eval_import_from`     |
| `class Evil(metaclass=Meta): ...`                           | class-keyword-args gate in `eval/classes.rs::eval_class_def`    |
| `input(...)`                                                | builtin blocked (`eval/functions/builtins.rs`)                  |
| `[1] * 10**9` (huge list repeat)                            | `MAX_COLLECTION_SIZE` in `eval/operations.rs`                   |
| `while True: pass`                                          | `max_while_iterations`                                          |
| `def f(): return f()` (infinite recursion)                  | `max_recursion_depth`                                           |
| Tight Python loop without tools                             | `max_operations` (and optional cooperative wall-clock)          |
| Oversized state import                                      | `MAX_IMPORT_SIZE` in `serialize.rs`                             |
| State-blob from an earlier wire format                      | `STATE_FORMAT_VERSION` mismatch → `StateFormatSuperseded`       |
| State-blob tampering across resume boundaries               | Host responsibility (this crate does not sign blobs)            |

Rows that were previously in this table — `class Evil(SomethingTrusted)`, `@dataclass`, `case SomeClass(x, y)` — are no longer blocked, because the constructs themselves are supported as of Tracks B1/B2/B4. Their security comes from the orthogonal blocklists: any class body that tries to walk the class chain via `__bases__` / `__mro__` / `__subclasses__`, or reach interpreter internals via `__globals__` / `__code__` / `__dict__`, is still rejected (`BLOCKED_ATTRIBUTES`); any decorator that tries to call `eval` is still rejected (`DANGEROUS_NAMES`); any `match` arm cannot dereference a blocked dunder. `__class__` reads now resolve (they alias `type(x)`, which was always available), but that is a dead end without the `__bases__`/`__subclasses__` links, and `__class__` writes stay blocked. The user-facing construct is fine; the escape primitives are still gated.

## Out of scope

- **Side-channel attacks** (timing, cache-residency, power). The runtime does not promise constant-time behavior for arbitrary user code.
- **Slow but well-formed code.** Tool calls may take arbitrary time; the interpreter does not impose a wall-clock budget on awaited tool work.
- **Adversarial / harmful content in tool outputs.** If a tool returns a string, that string is text — applying policy to it is the upstream LLM/policy layer's job, not the interpreter's.
- **Third-party Python ecosystem CVEs.** The interpreter does not run CPython; it walks `rustpython-parser` AST nodes through its own evaluator, so CPython-specific vulnerabilities don't apply.
