class Comparable:
    def __init__(self, v):
        self.v = v
    def __eq__(self, o):
        return isinstance(o, Comparable) and self.v == o.v
    def __ne__(self, o):
        return not self.__eq__(o)
    def __lt__(self, o):
        return self.v < o.v
    def __hash__(self):
        return hash(self.v)
a, b = Comparable(1), Comparable(2)
print(a == b, a != b, a < b, a == Comparable(1))
d = {Comparable(1): "one", Comparable(2): "two"}
print(d[Comparable(1)])
print(Comparable(1) in d)
s = {Comparable(1), Comparable(1), Comparable(2)}
print(len(s))
class Boolable:
    def __init__(self, items):
        self.items = items
    def __bool__(self):
        return len(self.items) > 0
print(bool(Boolable([1, 2])), bool(Boolable([])))
print("yes" if Boolable([1]) else "no")
if Boolable([]):
    print("nonempty")
else:
    print("empty")
class LengthBool:
    def __init__(self, n):
        self.n = n
    def __len__(self):
        return self.n
print(bool(LengthBool(5)), bool(LengthBool(0)))
class Indexable:
    def __getitem__(self, key):
        if isinstance(key, slice):
            return f"slice({key.start},{key.stop},{key.step})"
        return f"item({key})"
i = Indexable()
print(i[5], i[1:10:2], i["key"])
class Context:
    def __enter__(self):
        return "resource"
    def __exit__(self, *args):
        return False
with Context() as r:
    print("in context:", r)
class Descriptor:
    def __set_name__(self, owner, name):
        self.name = name
    def __get__(self, obj, objtype=None):
        return f"value_of_{self.name}"
class Owner:
    attr = Descriptor()
print(Owner().attr)
class CallableClass:
    def __init__(self, factor):
        self.factor = factor
    def __call__(self, x):
        return x * self.factor
double = CallableClass(2)
print(double(5), double(10))
print([double(x) for x in range(3)])
class Container:
    def __init__(self, *items):
        self._items = list(items)
    def __contains__(self, x):
        return x in self._items
    def __iter__(self):
        return iter(self._items)
    def __len__(self):
        return len(self._items)
    def __reversed__(self):
        return reversed(self._items)
c = Container(1, 2, 3)
print(2 in c, 5 in c, len(c))
print(list(c), list(reversed(c)))
class Formatted:
    def __format__(self, spec):
        if spec == "upper":
            return "FORMATTED"
        return "formatted"
print(f"{Formatted()}", f"{Formatted():upper}", format(Formatted(), "x"))
