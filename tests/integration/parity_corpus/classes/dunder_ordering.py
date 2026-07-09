# Pins: user-class __lt__/__le__/__gt__/__ge__ dispatch from `<`/
# `<=`/`>`/`>=` and from sorted(). Reflected dispatch — `a < Box(x)`
# where only Box defines __gt__ — also fires.
class V:
    def __init__(self, n):
        self.n = n
    def __repr__(self):
        return f"V({self.n})"
    def __lt__(self, other):
        return self.n < other.n
    def __le__(self, other):
        return self.n <= other.n
    def __gt__(self, other):
        return self.n > other.n
    def __ge__(self, other):
        return self.n >= other.n

a, b, c = V(1), V(2), V(2)
print(a < b)
print(b < a)
print(b <= c)
print(b >= c)
print(a > b)
print(b > a)
print(sorted([V(3), V(1), V(2)]))
