---
id: parity-strptime-exception-mro-d-follow-up-g
title: "Parity: strptime + exception MRO (D follow-up + G)"
status: closed
priority: p2
dependencies: []
related: [epic-post-0-1-hardening-and-product-backlog]
scopes: [eval/modules, eval]
shared_scopes: []
paths: []
tags: [parity, eval/modules]
closed_reason: superseded
closed_note: Split into d-implement-datetime-strptime and g-full-exception-hierarchy-mro-and-cause-context-chaining
---
## Why
CONFORMANCE: strptime not implemented; exception hierarchy only hard-coded subsets.

## Work
- datetime.strptime for common directives (match strftime surface)
- Expand matches_exception_type toward user-class MRO (Track G)
- Corpus + CONFORMANCE status lines

## Done when
strptime corpus green for supported directives; STATUS D/G updated.
