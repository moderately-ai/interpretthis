---
id: gap-dict-equal-float-key-repr-fidelity
title: "Gap: dict key representation fidelity for equal int/float keys"
status: ready
priority: p3
dependencies: []
related: []
scopes: []
shared_scopes: []
paths: [crates/interpretthis/src/eval/literals.rs, crates/interpretthis/src/value.rs, crates/interpretthis/src/eval/render.rs, crates/interpretthis/tests/integration/parity_corpus/dicts/**]
tags: [gap, dicts, repr, parity]
---
Audit source comment: ValueKey folds integral floats to Int for lookup equality, so a standalone key like {2.0: 'x'} renders as {2: 'x'} whereas CPython preserves the first inserted key object for repr. Consider a stored-key vs lookup-key split or document this permanent cosmetic divergence.
