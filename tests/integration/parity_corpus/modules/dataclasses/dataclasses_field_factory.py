# `field(default_factory=...)` — fresh container per instance.
#
# Pins CPython semantics: a mutable container like `list` cannot be a
# bare default (CPython rejects `x: list = []` at decoration time),
# so the canonical pattern is `field(default_factory=list)`. Each
# constructed instance gets its own list — distinct identity, distinct
# mutation history.
from dataclasses import dataclass, field

@dataclass
class Bag:
    tags: list = field(default_factory=list)
    seen: dict = field(default_factory=dict)

a = Bag()
b = Bag()

# Distinct identities: mutating `a` does not affect `b`.
a.tags.append("x")
a.tags.append("y")
a.seen["k"] = 1

print(a.tags)
print(b.tags)
print(a.seen)
print(b.seen)
print(a == b)

# Constructor still accepts explicit values that override the factory.
c = Bag(tags=["preset"])
print(c.tags)
