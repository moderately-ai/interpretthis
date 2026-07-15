class Point:
    x = 0
    def __init__(self, x, y):
        self.x = x
        self.y = y
    def move(self):
        pass
p = Point(1, 2)
print(hasattr(p, "x"), hasattr(p, "z"))
print(getattr(p, "y"), getattr(p, "z", "default"))
setattr(p, "z", 99)
print(p.z)
print(isinstance(p, Point), isinstance(5, (int, str)))
print(callable(p.move), callable(5))
delattr(p, "z")
print(hasattr(p, "z"))
print(type(p) is Point)
print(issubclass(bool, int))
