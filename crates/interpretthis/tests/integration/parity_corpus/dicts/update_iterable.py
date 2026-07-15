d = {"a": 1}
d.update([("b", 2), ("c", 3)])
print(sorted(d.items()))
d.update((("d", 4),))
print(d["d"])
d.update({"e": 5})
print(d["e"])
d.update(zip("fg", [6, 7]))
print(d["f"], d["g"])
d.update([["h", 8]])
print(d["h"])
other = {"x": 10, "y": 20}
d2 = {}
d2.update(other.items())
print(sorted(d2.items()))
from collections import OrderedDict, Counter, defaultdict
d3 = {}
d3.update(OrderedDict([("a", 1)]))
d3.update(Counter("aab"))
print(sorted(d3.items()))
d4 = {"a": 1}
d4.update([("b", 2)], c=3, d=4)
print(sorted(d4.items()))
d5 = {}
d5.update()
print(d5)
try:
    {}.update([("a", 1, 2)])
except ValueError as e:
    print("VE", "length 3" in str(e) or "2 is required" in str(e))
dd = defaultdict(int)
dd.update([("x", 5)])
print(dict(dd))
