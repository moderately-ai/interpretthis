# Pins: user-class __eq__ dispatch from `==` / `!=` comparisons,
# `in`-set membership (via hash+eq), and dict-lookup equality.
class Pair:
    def __init__(self, a, b):
        self.a = a
        self.b = b
    def __eq__(self, other):
        return isinstance(other, Pair) and (self.a, self.b) == (other.a, other.b)
    def __hash__(self):
        return hash((self.a, self.b))

p1 = Pair(1, 2)
p2 = Pair(1, 2)
p3 = Pair(3, 4)
print(p1 == p2)
print(p1 == p3)
print(p1 != p2)
print(p1 != p3)
