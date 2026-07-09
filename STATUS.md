# Parity status — interpretthis

Living board for language and stdlib parity work.

**User-facing divergences and stable error anchors** stay in
[`CONFORMANCE.md`](./CONFORMANCE.md). This file is the progress table only —
what has shipped, what is partial, and what is still open.

| Track | Scope | Status |
|---|---|---|
| Foundation | Differential corpus runner, `STATE_FORMAT_VERSION`, CONFORMANCE / THREAT / MODULE_TEMPLATE | ✅ shipped |
| A0 — Type infrastructure | `TypeObject` + slot tables + builtin singletons | ✅ shipped |
| A1 — Hash/equality dispatch | `__eq__` / `__hash__` + bool↔int unification + user-class `__eq__` | ✅ shipped — async `op::compare` / `op::hash` / `op::eq`; dict/set/list membership + `list.count`/`index`/`remove` + `hash()` builtin; custom `__eq__` beyond structural fields covered |
| A2 — Ordering dispatch | `__lt__` / `__le__` / `__gt__` / `__ge__` / `__contains__` | ✅ shipped — `op::compare` / `op::lt` / `op::contains` + parity `dunder_ordering` / `dunder_len_and_contains` |
| A3 — Arithmetic dispatch | Binary / unary / augmented arith + reflected + `NotImplemented` | ✅ shipped — `op::binop` / iadd path + parity `dunder_arith` / `dunder_iadd` |
| A4 — Iteration dispatch | `__iter__` / `__next__` | ✅ shipped — `op::iter` + parity `dunder_iter_protocol` |
| A5 — Item-access dispatch | `__getitem__` / `__setitem__` / `__delitem__` / `__missing__` / `__len__` | ✅ shipped — `op::getitem`/`setitem`/`delitem`/`len` + parity `dunder_subscript` |
| A6 — Attribute dispatch + descriptors | `__getattr__` / `__setattr__` + `@property` | ✅ shipped — getattr/setattr/delattr intercepts + `@property`; full data descriptors still open (separate ticket) |
| B1 — Inheritance + MRO + `super()` | Multi-level inheritance, C3, `super()` | ✅ shipped |
| B2 — Class decorators | `@property` / `@staticmethod` / `@classmethod` / `@dataclass` | ✅ shipped |
| B3 — Counter as `dict` subclass | `__missing__` → 0, multiset arithmetic | ✅ shipped |
| B4 — Match class patterns | Positional + keyword via `__match_args__` | ✅ shipped |
| C — Generators | `yield` / `yield from`; iterator `next` / `send` / `throw` / `close` | ⚠️ partial — eager yield works; iterator methods pending |
| D — Datetime | `date` / `datetime` / `time` / `timedelta` / `timezone` + `strftime` / `strptime` | ✅ shipped |
| E — Stdlib expansion | hashlib, base64, textwrap, string, itertools, functools, collections, typing, enum, dataclasses, decimal, fractions, copy | ✅ shipped (follow-ups: richer `functools`, etc.) |
| F — `with` statement | `__enter__` / `__exit__` | ✅ shipped (user-class managers; no `contextlib` module) |
| G — Exception hierarchy | Full tree + MRO matching + `__cause__` / `__context__` | ⚠️ partial — typed exceptions + hard-coded hierarchy subsets; full user-class MRO pending |

Legend: ✅ shipped · ⚠️ partial · ⏳ pending
