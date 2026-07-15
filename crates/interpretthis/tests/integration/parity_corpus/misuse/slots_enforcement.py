class Fixed:
    __slots__ = ("a", "b")
    def __init__(self, a, b):
        self.a = a
        self.b = b
f = Fixed(1, 2)
print(f.a, f.b)
f.a = 100
print(f.a)
try:
    f.c = 3
except AttributeError:
    print("no slot c")
class Point:
    __slots__ = ["x", "y"]
p = Point()
p.x = 10
p.y = 20
print(p.x + p.y)
try:
    p.z = 30
except AttributeError:
    print("no z")
class Inherited(Fixed):
    __slots__ = ("c",)
i = Inherited(1, 2)
i.c = 3
print(i.a, i.b, i.c)
