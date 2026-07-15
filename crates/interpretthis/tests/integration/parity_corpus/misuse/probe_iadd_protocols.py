class Accumulator:
    def __init__(self):
        self.items = []
    def __iadd__(self, other):
        self.items.append(other)
        return self
    def __repr__(self):
        return f"Acc({self.items})"
a = Accumulator()
a += 1
a += 2
print(a)
lst = [1, 2, 3]
original_id = id(lst)
lst += [4, 5]
print(lst, id(lst) == original_id)
s = "abc"
s += "def"
print(s)
d = {"a": 1}
d.update({"b": 2})
print(d)
class Counter2:
    def __init__(self, n=0):
        self.n = n
    def __isub__(self, x):
        self.n -= x
        return self
    def __repr__(self):
        return f"Counter2({self.n})"
c = Counter2(10)
c -= 3
print(c)
a = [1]
b = a
a += [2, 3]
print(b, a is b)
