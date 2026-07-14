def describe(x):
    match x:
        case 0:
            return "zero"
        case int() if x < 0:
            return "negative"
        case int():
            return "positive int"
        case str() as s:
            return f"string {s}"
        case [a, b]:
            return f"pair {a},{b}"
        case [a, *rest]:
            return f"list starting {a}"
        case {"type": t, "value": v}:
            return f"dict {t}={v}"
        case (x, y, z):
            return f"triple {x},{y},{z}"
        case _:
            return "other"


print(describe(0))
print(describe(-5))
print(describe(42))
print(describe("hi"))
print(describe([1, 2]))
print(describe([1, 2, 3, 4]))
print(describe({"type": "a", "value": 9}))
print(describe((1, 2, 3)))
print(describe(3.14))


class Point:
    __match_args__ = ("x", "y")
    def __init__(self, x, y):
        self.x = x
        self.y = y


def loc(p):
    match p:
        case Point(0, 0):
            return "origin"
        case Point(x, 0):
            return f"x-axis at {x}"
        case Point(0, y):
            return f"y-axis at {y}"
        case Point(x, y):
            return f"point {x},{y}"


print(loc(Point(0, 0)), loc(Point(5, 0)), loc(Point(0, 3)), loc(Point(2, 4)))
