from typing import NamedTuple
class Point(NamedTuple):
    x: int
    y: int
    label: str = "origin"
p = Point(1, 2)
print(p.x, p.y, p.label)
print(p)
q = Point(3, 4, "custom")
print(q.label)
print(p._asdict())
print(p._replace(x=10))
print(Point._fields)
print(p[0], p[1])
a, b, c = p
print(a, b, c)
print(len(p))
print(p == Point(1, 2))
