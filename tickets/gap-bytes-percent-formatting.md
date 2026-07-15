---
id: gap-bytes-percent-formatting
title: "Gap: bytes %-formatting (b'%d' % x) not implemented"
status: ready
priority: p3
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/operations.rs, crates/interpretthis/src/eval/strings.rs]
tags: [gap, bytes, formatting]
---
`bytes % args` (printf-style bytes formatting) raises
`TypeError: unsupported operand type(s) for %: 'bytes' and 'tuple'`.
CPython supports it: `b"%d-%s" % (42, b"x")` -> `b'42-x'`.

`mod_values` in operations.rs routes `str % args` to
`strings::str_percent_format` but has no `Value::Bytes` arm. Implementing it
means a bytes analogue of `str_percent_format` with bytes-specific conversions:
`%b`/`%s` expect a bytes-like object (or one with `__bytes__`), `%a`/`%r` insert
the ascii/repr of the object, and `%d`/`%i`/`%x`/`%o`/`%f`/`%e`/`%c`/`%%` mirror
the str codes but emit bytes. A str argument to `%s` raises (bytes formatting
does not accept str). Non-trivial because the format string and every piece of
output are bytes, not text — decoding to str and reusing the str path would be
incorrect for non-ASCII data.

Lower priority: `.decode().format()` / f-strings cover most real formatting; raw
bytes %-formatting is used mainly in wire-protocol code.
