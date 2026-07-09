# Pins: @dataclass(frozen=True) and order=True.
from dataclasses import dataclass

@dataclass(frozen=True)
class Point:
    x: int
    y: int

p = Point(1, 2)
print(p.x, p.y)
try:
    p.x = 9
except Exception as e:
    print(type(e).__name__)

@dataclass(order=True)
class Score:
    points: int
    name: str

a = Score(10, "a")
b = Score(20, "b")
c = Score(10, "z")
print(a < b)
print(a < c)
print(a > c)
print([s.points for s in sorted([b, a, c])])
