def m(x):
    match x:
        case [1, 2, *rest]: return f"starts12 rest={rest}"
        case [*init, last]: return f"init={init} last={last}"
        case {"type": "point", "x": px, "y": py}: return f"point {px},{py}"
        case {"type": t, **rest}: return f"type={t} rest={rest}"
        case (a, b) | [a, b]: return f"pair {a} {b}"
        case str() | bytes() as s: return f"stringy {s!r}"
        case int(n) if n > 100: return f"big {n}"
        case 0 | 1 | 2: return "small"
        case _: return "other"
print(m([1, 2, 3, 4]))
print(m([1, 2]))
print(m([10, 20, 30]))
print(m({"type": "point", "x": 5, "y": 6}))
print(m({"type": "circle", "r": 3}))
print(m((7, 8)))
print(m("hi"))
print(m(b"bytes"))
print(m(500))
print(m(1))
print(m(3.14))
class Point:
    __match_args__ = ("x", "y")
    def __init__(self, x, y): self.x, self.y = x, y
def locate(p):
    match p:
        case Point(x=0, y=0): return "origin"
        case Point(0, y): return f"yaxis {y}"
        case Point(x, 0): return f"xaxis {x}"
        case Point(x, y) if x == y: return f"diagonal {x}"
        case Point(): return "point"
        case _: return "notpoint"
print(locate(Point(0, 0)), locate(Point(0, 5)), locate(Point(3, 0)))
print(locate(Point(4, 4)), locate(Point(1, 2)), locate(42))
def nested(d):
    match d:
        case {"user": {"name": n, "roles": [first, *_]}}: return f"{n}/{first}"
        case {"user": {"name": n}}: return f"{n}/norole"
        case _: return "?"
print(nested({"user": {"name": "alice", "roles": ["admin", "user"]}}))
print(nested({"user": {"name": "bob"}}))
match [1, [2, 3], {"k": 4}]:
    case [a, [b, c], {"k": d}]: print(a, b, c, d)
