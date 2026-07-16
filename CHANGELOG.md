# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Post-0.4 hardening: closing CPython parity gaps found by widening the differential
parity corpus, plus a sandbox-robustness pass over the host-binding, resource-limit,
and state-resume seams. No new host-boundary surface.

### Security / robustness

- **Sandbox-constructible inputs can no longer abort the embedding host process**
  (an uncatchable `SIGSEGV`/`SIGABRT` is an effect outside the sandbox):
  - Value↔host conversion in both bindings now caps recursion depth and grows the
    stack, so a self-referential (`a = []; a.append(a)`) or deeply nested value
    raises a clean error instead of overflowing the FFI stack. The Node `DateTime`
    conversion uses checked arithmetic (chrono's `-` panics out of range).
  - `range()` materialisation, `bytes(n)`/`bytearray(n)`, and integer `**` are now
    bounded *before* allocating, so `list(range(10**18))`, `bytes(10**18)`, and
    `(2**1000000)**1000000` raise rather than OOM-abort.
  - Sizing and dropping a deeply-nested value grow the stack instead of overflowing.
- **Closed the in-place-growth memory-accounting holes** that let a loop evade the
  memory limit and OOM the host: `xs += ys` / `s |= t` (accounted a zero delta by
  comparing the grown handle to itself), `Counter.update`/`subtract`, and the
  defaultdict aug-assign pre-touch (`dd[i] += 1`) all now charge their growth.
- THREAT_MODEL.md documents the state-resume trust boundary precisely: import
  fails closed on the size/version gate but trusts blob *contents*, so untrusted
  blobs must be host-signed (no host-escape primitive is reachable regardless).

### Performance

- Memory sizing of lists and dicts is now O(1) per assignment/mutation via a
  cached byte size carried on the container, instead of re-walking the whole
  value each time. Loop-built deep nesting (`a = [a]`) and appending a large
  nested container go from O(n²)/O(n) to O(n)/O(1); a 50 000-deep build that
  previously timed out now completes in ~0.1 s.

### Added

- `Decimal` total-order and fused arithmetic methods: `compare_total`,
  `copy_sign`, `fma`, `remainder_near`, and `adjusted` (finite operands), plus a
  standalone `decimal.Context(prec=…, rounding=…)` constructor. `to_integral_value`
  / `to_integral` / `to_integral_exact` now honour an explicit `rounding=` keyword.
- `Fraction.from_decimal` classmethod (exact conversion of a `Decimal`), alongside
  the existing `Fraction.from_float`.

### Changed

- `Decimal.copy_sign` now takes its sign from `-0`/`-Inf` operands (the sign lives
  on the value's kind, not its zero payload).
- `typing` runtime reprs match CPython: a `TypeVar`/`ParamSpec` reprs as `~T`, and
  `None` inside a generic alias reprs as `NoneType` (`Union[int, None]` →
  `typing.Union[int, NoneType]`). `Union[...]` compares equal regardless of member
  order (`Union[int, str] == Union[str, int]`).
- `x.__class__` is now readable (returns the type object, identical to `type(x)`);
  assigning to `__class__` remains blocked. No new host-boundary surface — the
  result only names classes already reachable in the sandbox.
- ASCII decode errors now carry CPython's full detail
  (`'ascii' codec can't decode byte 0xNN in position P: ordinal not in range(128)`)
  instead of a truncated message.

## [0.4.0] — 2026-07-13

### Added

- **Python bindings, shipping to PyPI as `interpretthis`.** A `cp311-abi3` wheel
  per platform (CPython >= 3.11), no runtime dependencies. Tools may be sync
  `def` or `async def`; `execute()` blocks, `await execute_async()` runs on the
  caller's event loop and schedules tool coroutines there. Exceptions subclass
  their builtin twin, so `except NameError` behaves as the name promises.
- **Node bindings, shipping to npm as `interpretthis`.** Prebuilt Node-API addons
  for macOS, Linux (glibc + musl), and Windows on x64 and arm64; Node 22+. Tools
  may be `function` or `async function`. `execute()` is async by necessity: a JS
  tool callback can only resolve while the event loop is free.
- `release-python.yml` and `release-npm.yml`: every artifact is built *and*
  install-smoked on a native runner, and publishing is gated behind a manual
  dispatch plus a protected environment with a required reviewer. A tag builds; it
  does not publish.
- `docs/platform-support.md` — the tier policy, and the rule it enforces: a
  registry must never advertise a platform CI does not build and smoke.
- `Value::to_key` — the public inverse of `ValueKey::to_value`, deriving the
  hashable dict/set key for a value (and `Err(TypeError: unhashable type)` for
  those that have none). Any host building a `Value::Dict` from outside the
  crate needs to construct keys; without this it would have to re-implement the
  evaluator's key coercion — notably the integral-float fold that keeps
  `{2: x}[2.0]` on one slot — and a second implementation would silently
  diverge.
- `Tools::try_insert` — registers a tool, returning `Err(ToolError)` instead of
  panicking when the name is a dangerous builtin (`eval`, `exec`, `os`, …).
  `Tools::insert` / `Tools::with` keep panicking and stay the ergonomic form for
  a fixed startup tool set. The fallible form exists for callers that cannot
  absorb a panic: language bindings (where the name arrives from Python or
  JavaScript and a panic would cross an FFI boundary) and hosts registering
  tools from user-supplied config.
- `NOTICE` documenting the one non-permissive transitive dependency
  (`malachite`, LGPL-3.0-only, reached via `rustpython-parser`) and how the
  LGPL-3.0 §4 relink obligation is met for distributed binaries. Inert for
  source distribution; it governs the wheels and Node addons.

### Changed

- **Repository is now a Cargo workspace.** The library moved from the repo root
  to `crates/interpretthis/`; the root manifest is a virtual workspace. This
  makes room for the Python (PyPI) and Node (npm) binding crates. No source or
  behaviour change — the published crate builds from the same files, and the
  `.crate` tarball differs only by dropping repo-root metadata (`deny.toml`,
  `rustfmt.toml`, CI config, and the top-level docs) that was previously swept
  in because the package root *was* the repo root.
- `cargo package` / `cargo publish` now need `-p interpretthis` (see
  `RELEASING.md`).
- `Cargo.lock` is now committed. The workspace will ship prebuilt binaries, and
  those must build from a pinned, auditable dependency graph.

## [0.3.0] — 2026-07-09

Post-0.2 hardening: true generator frames, eval stack, security/docs, and
parity polish. Closes the post-0.2 ticket epic.

### Added

- True generator suspend/resume (`Value::Generator`) for for-based bodies;
  `next` / `send` / `throw` / `close`; `yield from`; while-based gens stay eager
- `ExceptionGroup.subgroup` / `.split` with nested flatten
- `decimal.localcontext` (save/restore `prec`); per-interpreter decimal prec
  (no process-global atomics)
- `InterpreterConfig::max_int_bits` resource gate on large shifts
- Class `__slots__` inheritance across bases
- Metaclass `__prepare__(name, bases)` namespace seed; method tables restored
  after `type()` rebuild
- Non-data descriptor precedence (instance dict shadows `__get__`-only)
- BigInt-aware `<<` / `>>` with shift-size caps
- `copy.deepcopy` cycle memo (Arc identity)
- `cargo deny` config + CI job (advisories/licenses)
- Table-driven builtin method handlers in `method_dispatch`
- `ClassValue::new` defaults for synthetic classes
- Host-facing crate docs for int / ExceptionGroup / async / tool timeouts

### Changed

- `eval_stmt` / `eval_expr` box each match arm separately (~8-deep recursion on
  default test stacks vs ~3 before)
- ExceptionType construction unified (direct + indirect calls)
- State export round-trips BigInt and ExceptionGroup.exceptions
- CONFORMANCE / STATUS truth pass for 0.2+ surface
- Dependency-update process documented for PR-disabled workflow

### Notes

- Generator bodies that use `while` still use the eager Lazy buffer path
- True async/await remains unsupported

## [0.2.0] — 2026-07-09

Language-surface and stdlib parity expansion after the 0.1.0 extract.

### Added

- Arbitrary-precision ints (`Value::BigInt`) with i64 fast path and promotion
- `NotImplemented` singleton and reflected-op fallthrough for user dunders
- User data descriptors (`__get__` / `__set__` / `__delete__`) beyond `@property`
- PEP 487: `__init_subclass__`, `__set_name__`
- PEP 654: `ExceptionGroup` / `BaseExceptionGroup` and `try`/`except*`
- Class-body `__slots__` and `@dataclass(slots=True)` field allowlists
- `metaclass=` with `__new__` and three-arg `type(name, bases, dict)`
- Computed class bases; nested `del` attribute paths; slice augmented assignment
- Bounded builtins `getattr` / `setattr` / `delattr` with shared instance fields
- Generator protocol surface (`next` / `send` / `throw` / `close` on Lazy buffers)
- Method kwargs (CPython 3.12 keyword surface) across dispatch tables
- `decimal.getcontext` / `setcontext` with mutable `prec`
- `Decimal.from_float`; `Fraction` float construct and arith
- `functools.cmp_to_key`, `lru_cache` / `cache`
- `contextlib.nullcontext` and `suppress`
- `datetime.strptime`; namedtuple iteration/len
- `copy.copy` vs `copy.deepcopy` (shared vs independent nested storage)
- Unicode-aware `str.casefold`; `str.encode` latin-1 + strict ascii
- Tool wall-clock timeout from remaining `max_execution_time`
- Tool errors catchable as `Exception`
- List `@` matrix multiply (intentional extension; see CONFORMANCE)
- TypeObject `has_methods_table` wiring for method-bearing builtins

### Changed

- Crate-root re-export trim (deeper types under `interpretthis::value`)
- Integer power keeps exact `BigInt` results instead of OverflowError past i64
- Recursion smoke tests use shallower depths after per-frame stack growth

### Notes

- `async`/`await` remains unsupported (clear runtime error + CONFORMANCE)
- Full coroutine frames and true async I/O remain backlog

## [0.1.0] — 2026-07-09

Initial public release of `interpretthis`: a sandboxed Python AST interpreter
for untrusted and LLM-generated code.

### Added

- Sandboxed evaluator over `rustpython-parser` ASTs with host tool injection
- Resource limits (operations, memory, recursion, cooperative wall-clock)
- Allowlisted stdlib modules and blocked dangerous names/attributes
- Versioned interpreter state export/import (`STATE_FORMAT_VERSION`)
- Differential parity corpus against host Python 3.12
- Dual license: MIT OR Apache-2.0
- Conformance and threat-model documentation

### Notes

- Not an embedded CPython; language surface is intentional — see
  [`CONFORMANCE.md`](./CONFORMANCE.md) and [`THREAT_MODEL.md`](./THREAT_MODEL.md).

[0.3.0]: https://github.com/moderately-ai/interpretthis/releases/tag/v0.3.0
[0.2.0]: https://github.com/moderately-ai/interpretthis/releases/tag/v0.2.0
[0.1.0]: https://github.com/moderately-ai/interpretthis/releases/tag/v0.1.0
