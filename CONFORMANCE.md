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

Parity is asserted against **`python3.12.x`**. The choice is driven by stability, full PEP 634 match-statement support, and the absence of PEG-parser quirks that surfaced in earlier releases. CI provisions exactly that minor release; the differential corpus runner in `tests/integration/parity_corpus_runner.rs` invokes the host `python3` and byte-diffs stdout, so a minor-version drift on locally-installed Python can produce false negatives on float repr, dict iteration corners, or error messages.

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

`print(some_set)` and `repr(some_set)` iterate the set in **insertion order**, not CPython's hash-randomized order. Sets are stored as ordered hash sets internally; this is a deliberate determinism choice for differential testing.

CPython itself documents set iteration order as implementation-defined ("Sets are unordered" — language reference §3.1.4); relying on it is a CPython bug, not a portability target. Our determinism gives differential corpus tests a stable baseline without forcing every snippet through `sorted()`.

Corpus snippets that compare set output across the two interpreters should still use `sorted()` for clarity (the convention is documented under [`PYTHONHASHSEED=0` enforcement](#pythonhashseed-enforcement)), but the underlying determinism property is what makes set-printing tests stable in the first place.

**Rationale**: deterministic output is the foundation of byte-diff parity testing. CPython's own spec permits this divergence.

**Status**: Permanent divergence.

---

## Import allowlist
<a id="import-allowlist"></a>

Only a curated set of stdlib modules is importable; arbitrary `import` of any other name raises `ModuleNotFoundError`. The allowlist lives in `src/eval/modules/mod.rs` as the `MODULES` registry; it IS the single registration point for module availability.

Module surfaces are split into two tiers:

- **Auto-imported** (no `import` statement required, bound in the default namespace): `json`, `re`, `datetime`. Historical artefact of the initial release; the carve-out is closed and does not grow.
- **Require-import** (must appear in an `import` or `from ... import` statement): every other module shipped today — `math`, `statistics`, `collections`, `string`, `textwrap`, `base64`, `hashlib`, `itertools`, `functools`, `typing`, `enum`, `dataclasses`, `decimal`, `fractions`, `copy`.

Single source of truth: every module ships a `pub struct XModule;` + `impl Module for XModule` (per `MODULE_TEMPLATE.md`) and registers itself in the `static MODULES: LazyLock<HashMap<&'static str, &'static dyn Module>>` in `src/eval/modules/mod.rs`. There is no second list — the registry IS the allowlist; `is_known_module` reads from it directly.

The allowlist is **closed by default**: an `import` of a name not in the registry raises a `ModuleNotFoundError`. Submodule imports (`import foo.bar`), `from <name> import *`, and relative imports remain unsupported regardless of registry membership; those rejections live in `src/eval/modules/mod.rs`.

Cross-link: [`THREAT_MODEL.md`](./THREAT_MODEL.md) covers the attack framing (`__import__('os').system(...)`, etc.).

**Rationale**: arbitrary `import` is a sandbox-escape primitive. The allowlist is the security boundary; it is not a performance or convenience knob.

**Status**: Permanent policy. The list itself grows as new modules are reviewed and added per `MODULE_TEMPLATE.md`.

---

## Blocked dunders and attributes
<a id="blocked-dunders"></a>

The following attribute names are rejected by `src/security/validator.rs` regardless of the object they're accessed on: `__class__`, `__bases__`, `__subclasses__`, `__mro__`, `__globals__`, `__code__`, `__closure__`, `__dict__`, `__builtins__`, `__spec__`, `__loader__`. Accessing any of them raises `InterpreterError::Security`.

Single-underscore names (`obj._field`) are **allowed** — in Python they are a naming convention, not a sandbox boundary. Only the explicit `BLOCKED_ATTRIBUTES` list is gated (see `src/security/names.rs`).

These dunders form the standard CPython sandbox-escape chain — `().__class__.__bases__[0].__subclasses__()` walks from any object to every loaded class.

Cross-link: [`THREAT_MODEL.md`](./THREAT_MODEL.md) documents validator entry points and attack patterns.

**Rationale**: closed by default. Any future addition of a name to the allowlist requires a security review and a new entry here naming the carve-out and its justification.

**Status**: Permanent policy.

---

## `eval` / `exec`
<a id="eval-exec"></a>

`eval`, `exec`, and `compile` are in the `DANGEROUS_NAMES` set at `src/security/names.rs` and cannot be referenced from user code. Calling them raises `InterpreterError::Security`. The same applies to `__import__`, `getattr`, `setattr`, `delattr`, `globals`, `locals`, `vars`, `dir`, `open`, `file`, `os`, `sys`, `subprocess`, and `shutil`. The const in `src/security/names.rs` is the source of truth.

`getattr` / `setattr` / `delattr` will eventually be re-introduced as bounded forms (three-arg `getattr(o, "name", default)` is on Track A6's roadmap); when that lands, the validator will permit the bounded form and continue rejecting the unbounded one. Until then the blanket rejection holds.

Cross-link: `THREAT_MODEL.md` enumerates the attack patterns these blocks defeat (`__import__('os').system(...)`, `().__class__.__bases__[0].__subclasses__()`, etc.).

**Rationale**: `eval` and `exec` parse arbitrary strings as code; they are sandbox escape primitives by definition.

**Status**: Permanent divergence for `eval` / `exec` / `compile`. `getattr` / `setattr` / `delattr` are planned to gain bounded variants without lifting the unbounded rejection.

---

## Out-of-scope language features
<a id="unsupported-language-features"></a>

The following language features are not supported and produce a clear error referencing this section:

- **`async` / `await`** — no coroutines, no `async def`, no `async for`, no `async with`. The evaluator is synchronous (modulo Tokio at the tool-call boundary).
- **Metaclasses** — `class Foo(metaclass=Meta):` is rejected. Metaclass-based DSLs are not a target use case.
- **Full descriptor protocol beyond `property`** — `@property` is supported (Track B2). User-defined data descriptors with arbitrary `__get__` / `__set__` / `__delete__` are not. Non-data descriptors beyond `@staticmethod` and `@classmethod` are not.
- **`__slots__`** — when written as `__slots__ = [...]` (`Stmt::Assign`) or `__slots__: list = [...]` (`Stmt::AnnAssign`), it is parsed and stored as a regular class attribute by `eval_class_def`; other statement forms in the class body fall through the catch-all and are silently dropped. In every case it has no effect on field storage (every instance still uses its own `BTreeMap` of fields). The performance benefit (no per-instance `__dict__`) does not apply to our object representation.
- **`__init_subclass__`** — not invoked on subclass creation.
- **`__set_name__`** — not invoked on attribute binding.

Most rejections produce an error of the form `<feature> is not supported (see CONFORMANCE.md#unsupported-language-features)`; `await` and the import gates follow that template. Some constructs still land on the generic `eval_stmt` catch-all (`InterpreterError::Runtime("unsupported statement: ...")`) without an anchor (`async def` / `async for` / `async with` / `except*`) — a documentation gap, not a security gap.

**Rationale**: these features add protocol surface that must be threat-modelled and benchmarked; they are out of scope until a concrete consumer needs them.

**Status**: Out of scope for now. A real consumer needing one of these files a follow-on issue with the specific use case; the line moves on evidence, not on principle.

---

## Exception groups (PEP 654)
<a id="exception-groups"></a>

`ExceptionGroup`, `BaseExceptionGroup`, and the `except*` syntax (introduced in CPython 3.11) are not supported. `rustpython-parser` accepts `except*` syntax and produces a `Stmt::TryStar` AST node; `eval_stmt` does not have a `Stmt::TryStar` arm, so the construct falls through to the catch-all and raises `InterpreterError::Runtime("unsupported statement: ...")` at eval time. `ExceptionGroup` is not registered as a builtin type.

The standard exception hierarchy (`BaseException` → `Exception` → `LookupError` → `KeyError`, etc.) is **partially** supported today. The interpreter raises typed exceptions of the right `type_name`, supports `try` / `except ExceptionName` matching by name, treats `Exception` as a universal catch-all, and walks a hard-coded hierarchy table in `src/eval/exceptions.rs::matches_exception_type` that covers three subtrees: `LookupError` catches `KeyError` / `IndexError`; `ArithmeticError` catches `ZeroDivisionError` / `OverflowError`; `OSError` catches `FileNotFoundError` / `IOError`. Full MRO traversal for user-defined exception classes is pending (Track G). Tracked under the parity program (Track G).

**Rationale**: PEP 654 is a niche feature with significant implementation surface (group splitting, exception-group exception chaining semantics, dedicated traceback presentation). No target consumer uses it today.

**Status**: Exception groups (PEP 654) — out of scope. File a follow-on if a real consumer needs them. Full exception hierarchy — planned (Track G).

---

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

**Rationale**: avoid the binary-float-expansion surprise. CPython's `Decimal.from_float(0.1)` is the explicit-opt-in form for users who want the expansion; we do not yet expose `from_float`.

**Status**: Deliberate divergence. Lift if a real consumer needs `Decimal.from_float`-shape behaviour.

---

## `Fraction` rejects `float`
<a id="fraction-float-rejection"></a>

`Fraction(0.1)` raises `TypeError`. CPython accepts a `float` here (constructing the exact rational that backs the binary-float value); we deliberately reject because the conversion's behaviour (`Fraction(0.1) == Fraction(3602879701896397, 36028797018963968)`) surprises users in the same way `Decimal(0.1)` does. Pass a string: `Fraction("1/10")`.

`Fraction` arithmetic with a `float` also raises `TypeError` (no float coercion). The `fraction_arith` slot in `src/types.rs` returns `None` on float operands intending to defer to the float side, but the float `numeric_arith` slot's `is_numeric` guard only matches `Int`/`Float`/`Bool` — not `Fraction` — so the dispatcher falls through to the "unsupported operand type(s)" TypeError. CPython produces a `float` result for `Fraction(1, 3) + 0.5`; we do not.

**Rationale**: avoids a known surprise on the constructor. The arithmetic divergence is a side-effect of how the dispatch slot tables are populated today — closing it requires teaching `numeric_arith` to lift `Fraction` to `float` on mixed-type operations.

**Status**: Divergence from CPython on both construction and arithmetic-with-float. Pass strings or pre-convert to `float` explicitly.

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

Binding uses `bind_method_params` in `src/eval/functions/method_dispatch.rs`.

**Status**: Shipped for the CPython 3.12 keyword surface above. Additional methods gain named kwargs on demand when CPython accepts them.

---

## Namedtuple iteration
<a id="namedtuple-iteration"></a>

`for x in nt`, `list(nt)`, unpacking, and `len(nt)` work on `collections.namedtuple` instances: `op::iter` / `op::len` materialise field values in `_fields` declaration order (same marker used for `nt[i]` subscript). Attribute access (`nt.x`) and subscript remain supported.

**Status**: Shipped.
