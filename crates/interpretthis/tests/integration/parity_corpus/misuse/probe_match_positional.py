from dataclasses import dataclass
@dataclass
class Point:
    x: int
    y: int
def f(p):
    match p:
        case Point(0, 0):
            return "origin"
        case Point(x, 0):
            return f"x={x}"
        case Point(0, y):
            return f"y={y}"
        case Point(x, y):
            return f"{x},{y}"
print(f(Point(0, 0)))
print(f(Point(3, 0)))
print(f(Point(0, 5)))
print(f(Point(2, 4)))
match [1, 2, 3]:
    case [a, b, c]:
        print(a, b, c)
match {"name": "Bob", "age": 30}:
    case {"name": n, "age": a}:
        print(n, a)
match (1, 2):
    case (x, y):
        print(x + y)
