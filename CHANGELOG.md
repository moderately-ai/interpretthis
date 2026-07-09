# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0]: https://github.com/moderately-ai/interpretthis/releases/tag/v0.1.0
