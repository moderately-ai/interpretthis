from dataclasses import dataclass, field
@dataclass
class Point:
    x: int
    y: int = 0
    tags: list = field(default_factory=list)
p = Point(1)
print(p)
print(p.x, p.y, p.tags)
p.tags.append("a")
q = Point(1)
print(q.tags)
@dataclass(frozen=True)
class Frozen:
    val: int
f = Frozen(42)
print(f.val)
try:
    f.val = 10
except Exception as e:
    print(type(e).__name__)
@dataclass(order=True)
class Ver:
    major: int
    minor: int
print(Ver(1, 2) < Ver(1, 3))
print(sorted([Ver(2, 0), Ver(1, 5), Ver(1, 0)]))
print(Ver(1, 0) == Ver(1, 0))
