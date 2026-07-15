# User-class protocol dunders: getitem/setitem/delitem, len, contains, iter, call.
class Vec:
    def __init__(self, data):
        self.data = list(data)
    def __getitem__(self, i):
        if isinstance(i, slice):
            return Vec(self.data[i])
        return self.data[i]
    def __setitem__(self, i, v):
        self.data[i] = v
    def __delitem__(self, i):
        del self.data[i]
    def __len__(self):
        return len(self.data)
    def __contains__(self, x):
        return x in self.data
    def __iter__(self):
        return iter(self.data)
    def __repr__(self):
        return f"Vec({self.data})"

v = Vec([1, 2, 3, 4, 5])
print(v[0], v[-1], len(v))
print(v[1:4], list(v[::2]))
v[0] = 100
print(v[0])
del v[0]
print(v.data)
print(3 in v, 99 in v)
print([x * 2 for x in v])
print(sum(v), max(v), min(v))

class Adder:
    def __init__(self, n):
        self.n = n
    def __call__(self, x):
        return x + self.n

add5 = Adder(5)
print(add5(10), list(map(add5, [1, 2, 3])))

# __getattr__ / __setattr__ fallback
class Proxy:
    def __init__(self):
        object.__setattr__(self, "_store", {})
    def __getattr__(self, name):
        return self._store.get(name, f"no-{name}")
    def __setattr__(self, name, value):
        self._store[name] = value

p = Proxy()
p.x = 42
print(p.x, p.missing)

# __eq__ and __hash__
class Point:
    def __init__(self, x, y):
        self.x, self.y = x, y
    def __eq__(self, other):
        return isinstance(other, Point) and (self.x, self.y) == (other.x, other.y)
    def __hash__(self):
        return hash((self.x, self.y))

print(Point(1, 2) == Point(1, 2), Point(1, 2) == Point(1, 3))
print(len({Point(1, 2), Point(1, 2), Point(3, 4)}))
