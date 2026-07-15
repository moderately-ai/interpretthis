# User-class __hash__/__eq__ drive dict-key and set-member identity.
class Key:
    def __init__(self, k):
        self.k = k
    def __hash__(self):
        return hash(self.k)
    def __eq__(self, o):
        return isinstance(o, Key) and self.k == o.k
    def __repr__(self):
        return f"Key({self.k!r})"

d = {Key("a"): 1, Key("b"): 2}
print(d[Key("a")], Key("b") in d)
d[Key("a")] = 10
print(len(d), d[Key("a")])

s = {Key(1), Key(2), Key(1), Key(3)}
print(len(s), Key(2) in s)

# hash collision but not equal -> distinct entries
class Bad:
    def __init__(self, v):
        self.v = v
    def __hash__(self):
        return 42
    def __eq__(self, o):
        return isinstance(o, Bad) and self.v == o.v
    def __repr__(self):
        return f"Bad({self.v})"

s2 = {Bad(1), Bad(2), Bad(3)}
print(len(s2), Bad(2) in s2, Bad(9) in s2)
d2 = {Bad(i): i for i in range(5)}
print(len(d2), d2[Bad(3)])

# frozen dataclass is hashable
from dataclasses import dataclass

@dataclass(frozen=True)
class Point:
    x: int
    y: int

pts = {Point(1, 2), Point(1, 2), Point(3, 4)}
print(len(pts), Point(1, 2) in pts)
print({Point(0, 0): "origin"}[Point(0, 0)])

# unhashable instance in a set raises
class Mutable:
    __hash__ = None

try:
    {Mutable()}
except TypeError as e:
    print("unhashable")
