# `match` class pattern against a dataclass.
#
# Pins CPython semantics: @dataclass synthesizes __match_args__ so PEP
# 634 patterns can destructure positionally. Keyword patterns also
# work, naming fields directly.
from dataclasses import dataclass

@dataclass
class Point:
    x: int
    y: int

@dataclass
class Circle:
    center: Point
    radius: int

def describe(shape):
    match shape:
        case Point(0, 0):
            return "origin"
        case Point(x, 0):
            return f"x-axis@{x}"
        case Point(0, y):
            return f"y-axis@{y}"
        case Point(x=x, y=y):
            return f"point({x},{y})"
        case Circle(center=Point(0, 0), radius=r):
            return f"unit-circle-r{r}"
        case Circle(center=c, radius=r):
            return f"circle@{c.x},{c.y}/r{r}"
    return "unknown"

print(describe(Point(0, 0)))
print(describe(Point(5, 0)))
print(describe(Point(0, 7)))
print(describe(Point(3, 4)))
print(describe(Circle(Point(0, 0), 1)))
print(describe(Circle(Point(2, 3), 5)))
