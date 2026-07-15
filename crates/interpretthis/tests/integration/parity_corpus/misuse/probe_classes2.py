class Shape:
    count = 0
    def __init__(self, name):
        self.name = name
        Shape.count += 1
    @property
    def label(self):
        return f"Shape: {self.name}"
    @classmethod
    def total(cls):
        return cls.count
    @staticmethod
    def describe():
        return "a shape"
s1 = Shape("circle")
s2 = Shape("square")
print(s1.label)
print(Shape.total())
print(Shape.describe())
class Point:
    __slots__ = ["x", "y"]
    def __init__(self, x, y):
        self.x, self.y = x, y
    def __repr__(self):
        return f"Point({self.x}, {self.y})"
    def __eq__(self, o):
        return (self.x, self.y) == (o.x, o.y)
p = Point(1, 2)
print(p)
print(p == Point(1, 2))
class Base:
    def greet(self): return "base"
class Derived(Base):
    def greet(self): return "derived+" + super().greet()
print(Derived().greet())
print(isinstance(Derived(), Base))
print(issubclass(Derived, Base))
