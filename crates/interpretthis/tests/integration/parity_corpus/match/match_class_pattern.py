# Pins: match/case with class patterns (positional + keyword) and
# capture-with-wildcard. Heavy in modern customer code.
class Point:
    def __init__(self, x, y):
        self.x = x
        self.y = y

def classify(p):
    match p:
        case Point(x=0, y=0):
            return "origin"
        case Point(x=0, y=_):
            return "y-axis"
        case Point(x=_, y=0):
            return "x-axis"
        case Point():
            return "other"
        case _:
            return "not a point"

print(classify(Point(0, 0)))
print(classify(Point(0, 5)))
print(classify(Point(3, 4)))
print(classify(42))


def shape_area(shape):
    match shape:
        case ("circle", r):
            return 3.14 * r * r
        case ("rect", w, h):
            return w * h
        case _:
            return None

print(shape_area(("circle", 2)))
print(shape_area(("rect", 3, 4)))
print(shape_area(("unknown",)))
