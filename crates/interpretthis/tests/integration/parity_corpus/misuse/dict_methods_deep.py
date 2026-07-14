d = {"a": 1, "b": 2}
print(d.get("c", 99))
print(d.setdefault("c", 3))
print(d)
d.update({"d": 4}, e=5)
print(sorted(d.items()))
print(d.pop("z", "default"))
dd = dict.fromkeys(["x", "y"], 0)
print(dd)
from collections import defaultdict
dm = defaultdict(list)
dm["k"].append(1)
print(dict(dm))
