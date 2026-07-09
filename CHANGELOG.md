# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- Full coroutine frames, true async I/O, and cargo-deny are still backlog

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

[0.2.0]: https://github.com/moderately-ai/interpretthis/releases/tag/v0.2.0
[0.1.0]: https://github.com/moderately-ai/interpretthis/releases/tag/v0.1.0
