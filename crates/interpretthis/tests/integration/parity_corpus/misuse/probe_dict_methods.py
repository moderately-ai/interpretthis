d = {"a": 1, "b": 2, "c": 3}
print(d.get("a"), d.get("z", -1))
print(list(d.keys()), list(d.values()))
print(list(d.items()))
print(d.pop("a"))
print(d)
d.update({"d": 4})
print(d)
print(d.setdefault("e", 5))
print(d.setdefault("b", 99))
d2 = dict(x=1, y=2)
print(d2)
print(len(d))
d.clear()
print(d)
d3 = {"a": 1}
d3["b"] = 2
del d3["a"]
print(d3)
print({}.get("missing"))
d4 = {1: "a", 2: "b"}
print(d4.popitem())
combined = {"a": 1}
combined |= {"b": 2}
print(combined)
print(dict.fromkeys("abc"))
counts = dict()
for x in "aabbc":
    counts[x] = counts.get(x, 0) + 1
print(counts)
