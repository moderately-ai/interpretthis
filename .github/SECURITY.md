# Security Policy

## Supported versions

Security fixes are accepted against the latest published release of
`interpretthis` on crates.io / the default branch of this repository.

## Reporting a vulnerability

Please **do not** open a public issue for security vulnerabilities.

Report privately by either:

1. **GitHub Security Advisories** on this repository
   (Security → Advisories → New draft security advisory), or
2. **Email** [security@moderately.ai](mailto:security@moderately.ai)

Prefer private channels so maintainers can coordinate a fix before disclosure.

Include:

1. A description of the issue and its impact
2. Steps to reproduce (minimal Python snippet preferred)
3. Affected version / commit if known

You should receive an acknowledgement within a few business days.

## Scope notes

`interpretthis` is a **sandboxed AST interpreter**, not a general-purpose
Python runtime. See [`THREAT_MODEL.md`](../THREAT_MODEL.md) for the
intended attacker model (adversarial / prompt-injected LLM output) and
the constructs that are intentionally rejected or resource-bounded.

Reports that rely on host-injected tools (network, filesystem, process
spawning) are out of scope for this crate — those are host trust-boundary
issues. Reports that escape the sandbox *without* a host tool (e.g. via
blocked dunders, import of disallowed modules, or resource-limit bypass)
are in scope.
