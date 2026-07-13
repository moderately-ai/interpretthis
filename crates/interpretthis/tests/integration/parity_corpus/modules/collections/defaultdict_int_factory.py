# defaultdict(factory) materialises missing keys from factory().
# Pins the eval_subscript intercept + invoke_factory for Class
# factories (int, list).
import collections
d = collections.defaultdict(int)
print(d["missing"])       # 0 from int()
d["a"] += 1
d["a"] += 1
d["b"] += 1
print(sorted(d.items()))
# With list factory
groups = collections.defaultdict(list)
for label, value in [("a", 1), ("b", 2), ("a", 3)]:
    groups[label].append(value)
print(sorted(groups.items()))
# Pre-populated from a mapping
seed = collections.defaultdict(int, {"x": 5})
print(seed["x"])
print(seed["y"])          # 0
