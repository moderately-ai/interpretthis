# Pins: class-body __slots__ restricts attributes.
class Point:
    __slots__ = ("x", "y")
    def __init__(self, x, y):
        self.x = x
        self.y = y

p = Point(1, 2)
print(p.x, p.y)
try:
    p.z = 3
    print("no-error")
except AttributeError:
    print("attr-error")
