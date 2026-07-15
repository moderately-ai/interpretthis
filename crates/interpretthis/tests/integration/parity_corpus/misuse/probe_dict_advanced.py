d = {i: i**2 for i in range(5)}
print(d)
print({k: v for k, v in d.items() if v > 4})
print(sorted(d.items(), key=lambda x: -x[1]))
print(list(d.values()))
print(max(d, key=d.get))
merged = {**{"a": 1}, **{"b": 2}, **{"a": 3}}
print(merged)
print({v: k for k, v in {"a": 1, "b": 2}.items()})
counts = {}
for c in "mississippi":
    counts[c] = counts.get(c, 0) + 1
print(sorted(counts.items()))
d2 = dict.fromkeys("abc", [])
print(d2)
nested = {"a": {"x": 1}, "b": {"y": 2}}
print({k: list(v.keys())[0] for k, v in nested.items()})
print(dict(sorted({"c": 3, "a": 1, "b": 2}.items())))
d3 = {"a": 1, "b": 2, "c": 3}
print({k: v*10 for k, v in d3.items()})
print(len(d3), "a" in d3, 5 in d3.values())
d3.pop("b")
print(d3)
print(sorted(d3.keys() | {"z"}))
