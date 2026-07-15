import copy

a = [1, [2, 3], {"k": [4, 5]}]
b = copy.copy(a)
c = copy.deepcopy(a)
a[1].append(99)
print(b[1], c[1])
a[2]["k"].append(88)
print(b[2], c[2])

d = {"list": [1, 2], "nested": {"x": [3, 4]}}
dd = copy.deepcopy(d)
d["list"].append(3)
d["nested"]["x"].append(5)
print(dd["list"], dd["nested"]["x"])

# copy of tuples, sets
t = (1, [2, 3])
tc = copy.deepcopy(t)
t[1].append(4)
print(tc[1])

s = {1, 2, 3}
sc = copy.copy(s)
sc.add(4)
print(sorted(s), sorted(sc))

# deepcopy with cycles
lst = [1, 2]
lst.append(lst)
lc = copy.deepcopy(lst)
print(lc[0], lc[1], lc[2] is lc)

# custom __deepcopy__ / __copy__ dispatched at every level.
import copy as _copy


class Custom:
    def __init__(self, v):
        self.v = v

    def __deepcopy__(self, memo):
        return Custom(self.v * 100)

    def __copy__(self):
        return Custom(self.v + 1)

    def __repr__(self):
        return f"Custom({self.v})"


print(_copy.deepcopy(Custom(5)))
print(_copy.copy(Custom(5)))
print(_copy.deepcopy([Custom(1), Custom(2)]))
print(_copy.deepcopy({"a": Custom(3)}))
nested = {"outer": [Custom(7)]}
print(_copy.deepcopy(nested))
