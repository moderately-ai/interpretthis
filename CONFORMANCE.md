# Conformance — interpretthis

## How to use this doc

`interpretthis` is a Rust evaluator over `rustpython_parser` AST. It targets CPython parity for the language and stdlib subset documented here, and intentionally diverges in a small number of well-understood places. This file is the single source of truth for those divergences. Every other doc that mentions a divergence should link here, not restate it.

Prefer that user-visible `"…not supported"` errors end with `(see CONFORMANCE.md#<anchor>)` so readers have one place for *why*. Anchors below are stable — renaming one requires updating every string that points at it. There is no automated cross-link gate in this repository today; treat the convention as a review checklist.

Cross-link to [`THREAT_MODEL.md`](./THREAT_MODEL.md) is one-way: the threat model explains *why* security rejections exist; this file catalogues *what* the interpreter does or does not support.

## Parity-program track status

Living status of language/stdlib parity work lives in [`STATUS.md`](./STATUS.md).
This file remains the divergence catalogue and stable error anchors only.

---

## Table of contents

- [Reference Python version](#reference-python-version)
- [`PYTHONHASHSEED=0` enforcement](#pythonhashseed-enforcement)
- [Regex backtracking](#regex-backtracking)
- [Set print order](#set-print-order)
- [Import allowlist](#import-allowlist)
- [Blocked dunders and attributes](#blocked-dunders)
- [`eval` / `exec`](#eval-exec)
- [Out-of-scope language features](#unsupported-language-features)
- [Exception groups (PEP 654)](#exception-groups)
- [chrono strftime directive coverage](#strftime-directives)
- [`Decimal` rejects `float`](#decimal-float-rejection)
- [`Fraction` rejects `float`](#fraction-float-rejection)
- [`OrderedDict.move_to_end` on plain `dict`](#ordereddict-on-dict)
- [Method-call keyword arguments](#method-call-kwargs)
- [Namedtuple iteration](#namedtuple-iteration)

---

## Reference Python version
<a id="reference-python-version"></a>

Parity is asserted against **`python3.12.x`**. The choice is driven by stability, full PEP 634 match-statement support, and the absence of PEG-parser quirks that surfaced in earlier releases. CI provisions exactly that minor release; the differential corpus runner in `crates/interpretthis/tests/integration/parity_corpus_runner.rs` invokes the host `python3` and byte-diffs stdout, so a minor-version drift on locally-installed Python can produce false negatives on float repr, dict iteration corners, or error messages.

CI provisions `python3.12` via `actions/setup-python` (see `.github/workflows/ci.yml`). Locally, use a 3.12.x host `python3` for parity runs; a minor-version drift can produce false negatives on float repr or error wording.

When 3.13 becomes the pinned reference, bump CI, this section, and re-baseline the corpus. Until then, do not author 3.13-only snippets.

**Rationale**: pin one minor version so parity is a binary property, not a moving target.

**Status**: Permanent policy. The pinned version itself rolls forward.

---

## `PYTHONHASHSEED=0` enforcement
<a id="pythonhashseed-enforcement"></a>

The differential corpus runner sets `PYTHONHASHSEED=0` on every host `python3` subprocess. Without this, CPython's per-process hash randomization gives sets and (pre-3.7) dicts non-deterministic iteration order — a byte-diff against our deterministic insertion-ordered output would flake on roughly half of all set-printing snippets.

`PYTHONHASHSEED=0` is sufficient for dict and set ordering reproducibility across runs of the *same* CPython binary; it does **not** guarantee identical hashing across CPython versions. Corpus snippets that print sets must therefore wrap output in `sorted(...)` regardless:

```python
# wrong — relies on hash order
print({1, 2, 3})

# right — deterministic across versions and across our interpreter
print(sorted({1, 2, 3}))
```

The hygiene rule is repeated in `MODULE_TEMPLATE.md` "Common pitfalls" because every new module author hits it.

**Rationale**: byte-diffing stdout is the cheapest possible parity check; non-determinism in the reference output makes the cheap check impossible.

**Status**: Permanent policy.

---

## Regex backtracking
<a id="regex-backtracking"></a>

Rust's `regex` crate is linear-time by design: it rejects backreferences (`\1`, `\2`, ...) and lookaround assertions (`(?=...)`, `(?!...)`, `(?<=...)`, `(?<!...)`) at compile time. CPython's `re` accepts both. `interpretthis`'s `re` module compiles via `regex::Regex`, so patterns using these constructs raise `re.error` at compile time rather than matching with potentially-exponential complexity.

The supported subset covers the vast majority of LLM-generated extraction patterns: character classes, quantifiers (greedy and lazy), alternation, capturing groups, named groups, non-capturing groups, anchors, and flags via inline `(?i)` / `(?m)` / `(?s)` syntax.

**Rationale**: ReDoS in a sandbox running untrusted LLM-generated code is unacceptable. A worst-case-exponential regex engine is a denial-of-service primitive available to any prompt-injection attacker. The linear-time guarantee is a security property, not a performance preference.

**Status**: Permanent divergence.

---

## Set print order
<a id="set-print-order"></a>

`set`/`frozenset` iterate in **CPython's hash-table slot order**, not insertion order. Both are backed by a port of CPython's open-addressing table (`crates/interpretthis/src/pyset.rs`) keyed on the bit-exact `pyhash` hash: the probe sequence, resize thresholds, dummy-tombstone reuse, and per-operation presize/merge all match `Objects/setobject.c`, so construction, `|`/`&`/`-`/`^`, `.copy()`, mutation, and `pop()` reproduce CPython 3.12 (`PYTHONHASHSEED=0`) order byte-for-byte. Corpus snippets can therefore print a set directly instead of routing through `sorted()`.

Three residuals remain, all irreducible rather than convenience cuts:

- **Constant set/frozenset literals** (`{'a', 'b', 'c'}` — all-constant elements) fold through CPython's compiler (`frozenset(list(frozenset(source)))` + `SET_UPDATE`), reproduced at ~98%. The remaining ~2% of collision-heavy literals are non-deterministic **in CPython itself** — they depend on compile-time interning state we cannot observe.
- **Sets containing user instances** (or the numeric/temporal types `pyhash` does not reproduce) fall back to insertion order; CPython orders them by object address, which is not reproducible.
- **Float object identity.** CPython dedups a set with `is`-before-`==`, so two *distinct* `NaN` objects are two elements while `nan in {nan}` (the *same* object) is `True`. Our clone-on-load model gives floats no object identity, so `NaN`-containing sets diverge. This is the same identity limitation documented for `is` on uncached immutables.

**Rationale**: reproducing CPython's observable order is both a correctness win and a performance win (O(1) membership, O(n+m) algebra). The residuals are CPython-side non-determinism or a fundamental identity-model gap, not order choices.

**Status**: Order matches CPython; the three residuals above are permanent.

---

## Import allowlist
<a id="import-allowlist"></a>

Only a curated set of stdlib modules is importable; arbitrary `import` of any other name raises `ModuleNotFoundError`. The allowlist lives in `crates/interpretthis/src/eval/modules/mod.rs` as the `MODULES` registry; it IS the single registration point for module availability.

Module surfaces are split into two tiers:

- **Auto-imported** (no `import` statement required, bound in the default namespace): `json`, `re`, `datetime`. Historical artefact of the initial release; the carve-out is closed and does not grow.
- **Require-import** (must appear in an `import` or `from ... import` statement): every other module shipped today — `math`, `statistics`, `collections`, `string`, `textwrap`, `base64`, `hashlib`, `itertools`, `functools`, `typing`, `enum`, `dataclasses`, `decimal`, `fractions`, `copy`.

Single source of truth: every module ships a `pub struct XModule;` + `impl Module for XModule` (per `MODULE_TEMPLATE.md`) and registers itself in the `static MODULES: LazyLock<HashMap<&'static str, &'static dyn Module>>` in `crates/interpretthis/src/eval/modules/mod.rs`. There is no second list — the registry IS the allowlist; `is_known_module` reads from it directly.

The allowlist is **closed by default**: an `import` of a name not in the registry raises a `ModuleNotFoundError`. Submodule imports (`import foo.bar`), `from <name> import *`, and relative imports remain unsupported regardless of registry membership; those rejections live in `crates/interpretthis/src/eval/modules/mod.rs`.

Cross-link: [`THREAT_MODEL.md`](./THREAT_MODEL.md) covers the attack framing (`__import__('os').system(...)`, etc.).

**Rationale**: arbitrary `import` is a sandbox-escape primitive. The allowlist is the security boundary; it is not a performance or convenience knob.

**Status**: Permanent policy. The list itself grows as new modules are reviewed and added per `MODULE_TEMPLATE.md`.

---

## Blocked dunders and attributes
<a id="blocked-dunders"></a>

The following attribute names are rejected by `crates/interpretthis/src/security/validator.rs` regardless of the object they're accessed on: `__class__`, `__bases__`, `__subclasses__`, `__mro__`, `__globals__`, `__code__`, `__closure__`, `__dict__`, `__builtins__`, `__spec__`, `__loader__`. Accessing any of them raises `InterpreterError::Security`.

Single-underscore names (`obj._field`) are **allowed** — in Python they are a naming convention, not a sandbox boundary. Only the explicit `BLOCKED_ATTRIBUTES` list is gated (see `crates/interpretthis/src/security/names.rs`).

These dunders form the standard CPython sandbox-escape chain — `().__class__.__bases__[0].__subclasses__()` walks from any object to every loaded class.

Cross-link: [`THREAT_MODEL.md`](./THREAT_MODEL.md) documents validator entry points and attack patterns.

**Rationale**: closed by default. Any future addition of a name to the allowlist requires a security review and a new entry here naming the carve-out and its justification.

**Status**: Permanent policy.

---

## `eval` / `exec`
<a id="eval-exec"></a>

`eval`, `exec`, and `compile` are in the `DANGEROUS_NAMES` set at `crates/interpretthis/src/security/names.rs` and cannot be referenced from user code. Calling them raises `InterpreterError::Security`. The same applies to `__import__`, `globals`, `locals`, `vars`, `dir`, `open`, `file`, `os`, `sys`, `subprocess`, and `shutil`. The const in `crates/interpretthis/src/security/names.rs` is the source of truth.

`getattr` / `setattr` / `delattr` are **bounded builtins**: the attribute name must be a string and is checked against `BLOCKED_ATTRIBUTES` (class-walk dunders like `__class__` / `__bases__` stay forbidden). Three-arg `getattr(o, name, default)` returns the default only on `AttributeError`, never on a security rejection. Instance field storage is shared (`SharedFields`), so `setattr`/`delattr` mutate by identity like CPython.

Cross-link: `THREAT_MODEL.md` enumerates the attack patterns these blocks defeat (`__import__('os').system(...)`, `().__class__.__bases__[0].__subclasses__()`, etc.).

**Rationale**: `eval` and `exec` parse arbitrary strings as code; they are sandbox escape primitives by definition.

**Status**: Permanent divergence for `eval` / `exec` / `compile`. Bounded `getattr` / `setattr` / `delattr` shipped.

---

## Out-of-scope language features
<a id="unsupported-language-features"></a>

The following language features are not supported and produce a clear error referencing this section:

- **`async` / `await`** — no coroutines, no `async def`, no `async for`, no `async with`. The evaluator is synchronous (modulo Tokio at the tool-call boundary).
- **Arbitrary class-definition keywords beyond the supported `metaclass=` path** — CPython forwards these through metaclass / `__init_subclass__` machinery. Remaining parity work is tracked by `gap-class-keyword-arguments-init-subclass`.
- **Some class-pattern keyword / `__match_args__` shapes** — pattern matching supports the core cases used by the parity corpus, but keyword-pattern parity is tracked by `gap-pattern-matching-class-keyword-patterns`.

Most rejections produce an error of the form `<feature> is not supported (see CONFORMANCE.md#unsupported-language-features)`; `await` and the import gates follow that template. The missing automated cross-link gate is tracked by `gap-unsupported-error-anchor-gate`.

**Rationale**: these features add protocol surface that must be threat-modelled and benchmarked; they are out of scope until a concrete consumer needs them.

**Status**: Partial by design; each remaining gap has a ticket ID above.

---

## Exception groups (PEP 654)
<a id="exception-groups"></a>

`ExceptionGroup` / `BaseExceptionGroup` constructors, `try`/`except*` leaf
splitting, and `.subgroup()` / `.split()` shipped in the 0.2.x/0.3.x line.
Nested groups are flattened for matching APIs.

**Status**: Shipped for the documented surface; new edge-case parity gaps should get a dedicated ticket.


## chrono strftime directive coverage
<a id="strftime-directives"></a>

`datetime.strftime` (date / datetime / time variants) is implemented over `chrono`'s format strings, which are a superset of POSIX but **not** a strict superset of CPython's directive table. Locale-sensitive directives (`%c`, `%x`, `%X`) and IANA timezone-name directives (`%Z` for non-fixed offsets) are the principal gaps; CPython resolves these against the host locale and `tzdata`, which the interpreter deliberately does not expose.

`strptime` is implemented via chrono's format parser (`datetime.datetime.strptime` and module-level `datetime.strptime`). It always returns a naive `datetime` (date-only formats get `00:00:00`; time-only formats use date `1900-01-01` per CPython).

The currently supported subset on `strftime` / `strptime` covers what `chrono::NaiveDate` / `NaiveDateTime` / `NaiveTime` accept in their format strings. Common safe-on-all-locales directives: `%Y`, `%m`, `%d`, `%H`, `%M`, `%S`, `%f` (microseconds), `%A` / `%a` (weekday name — English), `%B` / `%b` (month name — English), `%j` (day-of-year), `%U` / `%W` (week number), `%z` (numeric UTC offset for aware datetimes), `%%` literal. Locale-sensitive `%c` / `%x` / `%X` / `%Z` (named timezone) raise a chrono error rather than producing locale-dependent output.

**Rationale**: strftime divergence is the single most common source of "looks right, is wrong" bugs in a date-formatting layer. Making the supported set explicit and unsupported directives loud-fail is cheaper than auditing every output.

**Status**: `strftime` and `strptime` shipped (Track D). Locale-sensitive directives permanently out of scope.

---

## `Decimal` rejects `float`
<a id="decimal-float-rejection"></a>

`decimal.Decimal(0.1)` raises `TypeError`. This is a **deliberate divergence** — CPython 3.12 accepts `Decimal(0.1)` and constructs the binary float's exact expanded value (`Decimal('0.1000000000000000055511151231257827021181583404541015625')`). We reject the conversion because that expanded value almost never matches the source literal the user typed, and silently producing the "real" value is the surprising-result trap. To construct a `Decimal` from a literal, pass a string: `Decimal("0.1")`.

`Decimal` ± / * / / / // on a `float` argument also raises `TypeError`. Our error message is `unsupported operand type(s) for arithmetic: 'Decimal' and 'float'` — CPython's message names the specific operator (`unsupported operand type(s) for +: 'decimal.Decimal' and 'float'`). The behaviour (rejection) is the same; the message wording is a minor divergence.

**Rationale**: avoid the binary-float-expansion surprise. CPython's `Decimal.from_float(0.1)` is the explicit-opt-in form for users who want the expansion; `Decimal.from_float` is the explicit opt-in (shipped).

**Status**: `Decimal(float)` still rejected (use strings for literals). `Decimal.from_float` shipped for explicit binary expansion.

---

## `Fraction` float conversion and arithmetic
<a id="fraction-float-rejection"></a>

`Fraction(float)` and mixed Fraction/float arithmetic are supported for the common CPython-aligned cases. Remaining arithmetic parity gaps are `%` and `**`, tracked by `gap-fraction-mod-pow-parity`.

**Status**: Constructor and mixed float arithmetic shipped; modulo/power follow-up tracked.

---

## `OrderedDict.move_to_end` on plain `dict`
<a id="ordereddict-on-dict"></a>

We model `OrderedDict` as a regular `Dict` (CPython's `dict` has been insertion-ordered since 3.7, so the distinction is observable only through `OrderedDict`-specific methods). The `move_to_end(key, last=True)` method is registered on the shared `dict` dispatch table, so calling `move_to_end` on a plain `dict` succeeds where CPython raises `AttributeError`.

The reverse direction (an actual `OrderedDict` missing a method CPython supports) does not exist — every `dict` method also works on the OrderedDict alias.

**Rationale**: a separate `Value::OrderedDict` variant would propagate through the dispatch layer and every method table for the gain of one AttributeError. The single-direction divergence is the cheaper place to absorb it.

**Status**: Minor divergence from CPython. Subject to revisit if a real consumer hits it.

---

## Method-call keyword arguments
<a id="method-call-kwargs"></a>

Method calls thread kwargs through `dispatch_method` → per-type dispatchers. Behaviour matches CPython 3.12's positional-only vs keyword-capable split:

- **Accept kwargs**: `str.split` / `rsplit` (`sep`, `maxsplit`), `str.encode` (`encoding`, `errors`), `str.expandtabs` (`tabsize`), `dict.update(**kwargs)`, `OrderedDict.move_to_end` (`key`, `last`), `list.sort` (`key`, `reverse` — special-cased in `eval_call`), `str.format` / `format_map` (free-form field names).
- **Positional-only** (unexpected kwargs → `TypeError`, never silent drop): most other methods, including `dict.get` / `pop` / `setdefault`, `str.replace` / `center` / `strip` / …, and list mutators.

Binding uses `bind_method_params` in `crates/interpretthis/src/eval/functions/method_dispatch.rs`.

**Status**: Shipped for the CPython 3.12 keyword surface above. Additional methods gain named kwargs on demand when CPython accepts them.

---

## Namedtuple iteration
<a id="namedtuple-iteration"></a>

`for x in nt`, `list(nt)`, unpacking, and `len(nt)` work on `collections.namedtuple` instances: `op::iter` / `op::len` materialise field values in `_fields` declaration order (same marker used for `nt[i]` subscript). Attribute access (`nt.x`) and subscript remain supported.

**Status**: Shipped.

---

## Integer power beyond i64
<a id="int-power-i64-overflow"></a>

CPython `int` is arbitrary-precision: `2**100` returns a big integer.
`interpretthis` stores small ints as `Value::Int(i64)` and promotes to
`Value::BigInt` when values leave the i64 range (literals, arithmetic,
power). Extremely large exponents (`> 1_000_000`) still raise
`OverflowError` as a resource guard.

**Status**: Shipped (hybrid i64 + BigInt).

---

## List matrix multiplication (`@`)
<a id="list-matmul"></a>

CPython raises `TypeError` for `list @ list`. interpretthis implements
2-D list-of-lists matrix multiply (numpy-like) as a convenience for
agent workloads that emit `@` without importing numpy.

**Status**: Intentional extension (not CPython-identical).

---

## copy.copy vs copy.deepcopy
<a id="copy-shallow-deep"></a>

With shared list/instance storage, `copy.copy` preserves nested mutable
identity (CPython-aligned shallow share) and `copy.deepcopy` allocates
independent nested storage.

**Status**: Shipped (aligned with CPython shallow/deep distinction).

---

## async / await reopen criteria
<a id="async-await-reopen"></a>

`async`/`await` and coroutine frames remain unsupported (runtime error with
CONFORMANCE anchor). Reopen only with a concrete host need, for example:

- awaiting tool futures without blocking a worker thread
- streaming generator-like async tools under resource limits

Until then, prefer host-side async around `Interpreter::execute`.

**Status**: Permanent for 0.x unless a consumer files the use case above.

---

## Decimal context subset
<a id="decimal-context-subset"></a>

`getcontext` / `setcontext` / `localcontext` and mutable `prec` are supported.
`rounding` is exposed as a field (default `ROUND_HALF_EVEN`) but trap flags and
alternate rounding modes are not fully modelled.

**Status**: Partial subset; traps/full rounding open.

---

## copy cycles and hooks
<a id="copy-cycles"></a>

`copy.deepcopy` uses an Arc-identity memo so mutual list/instance cycles
terminate (unit-tested). User `__copy__` / `__deepcopy__` hooks are not
invoked from the sync copy module path.

**Status**: Cycles handled; hooks deferred.
