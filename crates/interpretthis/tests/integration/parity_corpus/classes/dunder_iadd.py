# Pins: user-class __iadd__/__isub__/__imul__ for `x += y` etc.
# CPython falls back to __add__ when __iadd__ is undefined, so
# `total += item` still works for additive classes.
class Bag:
    def __init__(self):
        self.items = []
    def __iadd__(self, other):
        self.items.extend(other)
        return self
    def __repr__(self):
        return f"Bag({self.items!r})"

class Acc:
    def __init__(self, n):
        self.n = n
    def __add__(self, other):
        return Acc(self.n + (other.n if isinstance(other, Acc) else other))
    def __repr__(self):
        return f"Acc({self.n})"

b = Bag()
b += [1, 2]
b += [3]
print(b)

# Acc has __add__ but no __iadd__; `+=` must rebind to a new Acc.
a = Acc(1)
a += Acc(10)
a += 5
print(a)
