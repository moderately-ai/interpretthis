# dict.pop preserves the insertion order of the remaining keys. Regression: it
# used swap_remove, which moved the last entry into the popped slot.
d = {"a": 1, "b": 2, "c": 3, "d": 4}
print(d.pop("b"))
print(list(d))          # a c d, not a d c
print(list(d.items()))
d.pop("a")
print(list(d))          # c d

# Missing key still returns the default or raises KeyError.
print(d.pop("z", 99))
try:
    d.pop("z")
except KeyError:
    print("KeyError")

# `del` also preserves order across dict / Counter / defaultdict.
from collections import Counter, defaultdict

d2 = {"a": 1, "b": 2, "c": 3, "d": 4}
del d2["b"]
print(list(d2))

c = Counter({"a": 1, "b": 2, "c": 3, "d": 4})
del c["b"]
print(list(c))

dd = defaultdict(int)
dd["a"], dd["b"], dd["c"], dd["d"] = 1, 2, 3, 4
del dd["b"]
print(list(dd))

