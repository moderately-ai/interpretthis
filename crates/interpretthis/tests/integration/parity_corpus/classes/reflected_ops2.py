# Reflected numeric dunders when the left operand doesn't implement the op.
class Money:
    def __init__(self, n):
        self.n = n
    def __add__(self, other):
        return Money(self.n + (other.n if isinstance(other, Money) else other))
    def __radd__(self, other):
        return Money(other + self.n)
    def __mul__(self, other):
        return Money(self.n * other)
    def __rmul__(self, other):
        return Money(other * self.n)
    def __eq__(self, other):
        return isinstance(other, Money) and self.n == other.n
    def __repr__(self):
        return f"Money({self.n})"

print(Money(5) + 3, 3 + Money(5))
print(Money(5) * 2, 2 * Money(5))
print(sum([Money(1), Money(2), Money(3)]))

class Vec:
    def __init__(self, x, y):
        self.x, self.y = x, y
    def __add__(self, o):
        return Vec(self.x + o.x, self.y + o.y)
    def __sub__(self, o):
        return Vec(self.x - o.x, self.y - o.y)
    def __neg__(self):
        return Vec(-self.x, -self.y)
    def __abs__(self):
        return (self.x**2 + self.y**2) ** 0.5
    def __repr__(self):
        return f"Vec({self.x}, {self.y})"

print(Vec(1, 2) + Vec(3, 4), Vec(5, 5) - Vec(1, 2), -Vec(1, 2), abs(Vec(3, 4)))

# comparison dunders and total ordering
from functools import total_ordering

@total_ordering
class Ver:
    def __init__(self, v):
        self.v = v
    def __eq__(self, o):
        return self.v == o.v
    def __lt__(self, o):
        return self.v < o.v

print(Ver(1) < Ver(2), Ver(3) > Ver(2), Ver(2) <= Ver(2), Ver(3) >= Ver(4))
print(sorted([Ver(3), Ver(1), Ver(2)], key=lambda x: x.v) == [Ver(1), Ver(2), Ver(3)])
