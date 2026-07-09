# Pins: user `__eq__` applies inside list membership and count/index/remove.
class Pair:
    def __init__(self, a):
        self.a = a
    def __eq__(self, other):
        return isinstance(other, Pair) and self.a == other.a
    def __hash__(self):
        return hash(self.a)

p1 = Pair(1)
p2 = Pair(1)
print(p1 == p2)
print([p1, p2, Pair(2)].count(Pair(1)))
print(Pair(1) in [p1, p2])
print([p1, p2].index(Pair(1)))
xs = [p1, Pair(2), p2]
xs.remove(Pair(1))
print(len(xs), xs[0].a)
print((p1, p2).count(Pair(1)))
