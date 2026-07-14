from collections import defaultdict, Counter, OrderedDict
dd = defaultdict(int)
dd["x"] += 1
dd["y"] += 2
print(dict(dd))
c = Counter("aabbbc")
print(dict(c))
od = OrderedDict([("a", 1), ("b", 2)])
print(dict(od))
print(dict({"a": 1}, b=2))
print(dict([("a", 1), ("b", 2)]))    # still works from pairs
