class Point:
    __slots__ = ["x", "y"]
    def __init__(self, x, y):
        self.x = x
        self.y = y
p = Point(1, 2)
print(p.x, p.y)
p.x = 10
print(p.x)
try:
    p.z = 5
except AttributeError as e:
    print("AttributeError")
class Vec:
    __slots__ = ("a", "b", "c")
    def __init__(self):
        self.a = 1
        self.b = 2
        self.c = 3
v = Vec()
print(v.a + v.b + v.c)
