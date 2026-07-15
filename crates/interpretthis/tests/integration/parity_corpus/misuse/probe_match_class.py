from dataclasses import dataclass
@dataclass
class Point:
    x: int
    y: int
def describe(p):
    match p:
        case Point(x=0, y=0):
            return "origin"
        case Point(x=0, y=y):
            return f"y-axis at {y}"
        case Point(x=x, y=0):
            return f"x-axis at {x}"
        case Point(x=x, y=y):
            return f"point {x},{y}"
    return "?"
print(describe(Point(0, 0)))
print(describe(Point(0, 5)))
print(describe(Point(3, 0)))
print(describe(Point(2, 4)))
