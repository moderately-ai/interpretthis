d = {"a": 1, "b": 2, "c": 3}
print(d.popitem())
print(len(d))
d2 = {"x": 1}
print(d2.pop("x"))
print(d2.pop("y", "default"))
d3 = dict(a=1, b=2)
print(d3)
print({k: v for k, v in [("a", 1), ("b", 2)]})
print({k: v*2 for k, v in {"x": 1, "y": 2}.items() if v > 1})
merged = {**{"a": 1}, "b": 2, **{"c": 3}}
print(merged)
d4 = {1: "a", 2: "b"}
print(list(d4.items()))
print(dict(zip("abc", [1, 2, 3])))
nested = {"outer": {"inner": 42}}
print(nested["outer"]["inner"])
d5 = {}
d5.setdefault("key", []).append(1)
d5.setdefault("key", []).append(2)
print(d5)
