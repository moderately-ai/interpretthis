---
id: gap-datetime-timetuple-struct-time
title: "Gap: date/datetime timetuple() and isocalendar() need a struct_time type"
status: ready
priority: p3
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/modules/datetime.rs, crates/interpretthis/src/eval/functions/method_dispatch.rs, crates/interpretthis/src/value.rs]
tags: [gap, stdlib, datetime]
---
`date.timetuple()` / `datetime.timetuple()` (returning `time.struct_time`) and
`date.isocalendar()` (returning `datetime.IsoCalendarDate`) are unimplemented —
they raise `AttributeError`. CPython returns named-tuple-like objects that
support all three of: integer indexing (`t[7]`), named attribute access
(`t.tm_yday`), and a distinctive repr (`time.struct_time(tm_year=..., ...)`).

A faithful implementation needs a value that provides all three. The blocker is
that the method-dispatch handlers (`date_methods`, `datetime_methods` in
method_dispatch.rs) are stateless fn-pointers with signature
`(&mut Value, &str, &[Value], &IndexMap) -> MethodOutcome`, so they cannot
register a namedtuple class in `state.classes` (how every other namedtuple is
represented). Options:

1. Add a dedicated `Value::StructTime`-style variant (or a generic
   "static namedtuple" value carrying field names + values) with its own
   get_item / get_attr / repr slots — no state required.
2. Thread `&mut InterpreterState` through the builtin method-dispatch handlers
   so these can build a real registered namedtuple.

Returning a plain `Value::Tuple` is NOT acceptable: it would silently diverge on
`repr()` and on `.tm_*` attribute access (a silent-wrong-value, the exact class
of bug the crate is being hardened against), so the feature is deferred until it
can be done with full fidelity.

Regression probe to add once implemented: `date(2026,12,31).timetuple().tm_yday`
(== 365), indexing, and repr.
