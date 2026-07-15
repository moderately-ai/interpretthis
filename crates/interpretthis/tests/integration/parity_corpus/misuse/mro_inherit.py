class A:
    def who(self):
        return "A"
class B(A):
    def who(self):
        return "B" + super().who()
class C(A):
    def who(self):
        return "C" + super().who()
class D(B, C):
    def who(self):
        return "D" + super().who()
print(D().who())
class Base:
    def __init__(self):
        self.items = []
class Mixin:
    def add(self, x):
        self.items.append(x)
class Combined(Base, Mixin):
    pass
c = Combined()
c.add(1)
c.add(2)
print(c.items)
print(type(D()).__name__)
