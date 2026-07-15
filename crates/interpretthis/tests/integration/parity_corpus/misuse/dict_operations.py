d = {"a": 1, "b": 2, "c": 3}
print(d.get("a"), d.get("z", 0))
print("a" in d, "z" in d)
d["d"] = 4
print(len(d))
del d["a"]
print(sorted(d.keys()))
d.update({"e": 5, "f": 6})
print(len(d))
print(d.pop("b"))
print(d.pop("x", None))
print({**d})
d2 = dict(zip("xyz", [1, 2, 3]))
print(d2)
print(dict.fromkeys("abc", 0))
merged = {**{"a": 1}, **{"b": 2}, **{"a": 10}}
print(merged)
counts = {}
for c in "hello":
    counts[c] = counts.get(c, 0) + 1
print(sorted(counts.items()))
inverted = {v: k for k, v in {"a": 1, "b": 2}.items()}
print(inverted)
nested = {"x": {"y": {"z": 1}}}
nested["x"]["y"]["z"] = 2
print(nested)
