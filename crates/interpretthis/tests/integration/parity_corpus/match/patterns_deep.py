def describe(x):
    match x:
        case 0:
            return "zero"
        case int() if x < 0:
            return f"neg-int {x}"
        case int():
            return f"pos-int {x}"
        case str() as s:
            return f"str {s}"
        case [a, b]:
            return f"pair {a},{b}"
        case [a, *rest]:
            return f"list-head {a} rest {rest}"
        case {"type": t, "val": v}:
            return f"dict {t}={v}"
        case (a, b, c):
            return f"triple {a},{b},{c}"
        case _:
            return "other"

print(describe(0), describe(-5), describe(42), describe("hi"))
print(describe([1, 2]), describe([1, 2, 3, 4]), describe({"type": "x", "val": 9}))
print(describe((1, 2, 3)), describe(3.14))

# Class patterns with positional and keyword
from dataclasses import dataclass

@dataclass
class Point:
    x: int
    y: int

def loc(p):
    match p:
        case Point(x=0, y=0):
            return "origin"
        case Point(x=0, y=y):
            return f"y-axis {y}"
        case Point(x=x, y=0):
            return f"x-axis {x}"
        case Point(x=x, y=y):
            return f"point {x},{y}"

print(loc(Point(0, 0)), loc(Point(0, 5)), loc(Point(3, 0)), loc(Point(2, 4)))

# Or-patterns and value capture
def classify(cmd):
    match cmd.split():
        case ["go", ("north" | "south" | "east" | "west") as direction]:
            return f"move {direction}"
        case ["look"]:
            return "looking"
        case ["take", *items]:
            return f"take {items}"
        case _:
            return "unknown"

print(classify("go north"), classify("look"), classify("take sword shield"), classify("xyz"))

# Nested patterns
match {"user": {"name": "alice", "roles": ["admin", "user"]}}:
    case {"user": {"name": name, "roles": [first, *_]}}:
        print(f"{name} is {first}")
