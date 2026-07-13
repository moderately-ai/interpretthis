# Pins: @dataclass(slots=True) rejects undeclared attributes.
from dataclasses import dataclass

@dataclass(slots=True)
class Point:
    x: int
    y: int

p = Point(1, 2)
print(p.x, p.y)
print(hasattr(Point, "__slots__"))
try:
    p.z = 3
    print("no-error")
except AttributeError:
    print("attr-error")
