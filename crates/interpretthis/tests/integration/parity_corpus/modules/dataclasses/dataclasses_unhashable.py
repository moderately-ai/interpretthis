# Default @dataclass (eq=True, frozen=False) makes instances
# unhashable — CPython explicitly sets `__hash__ = None` because
# value-based equality without immutability would silently break
# hash-dict invariants (mutating a key changes the hash, leaving the
# entry stranded in the wrong bucket).
from dataclasses import dataclass

@dataclass
class Movable:
    x: int

m = Movable(1)
try:
    hash(m)
    print("hashable")
except TypeError:
    print("unhashable")

try:
    {m: 1}
    print("dict-keyable")
except TypeError:
    print("dict-rejects")
