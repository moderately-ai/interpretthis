d = {"a": 1, "b": 2}
print(d | {"c": 3})
print({**d, "a": 10})
print(dict.fromkeys(["x","y"], 0))
print({1,2,3} ^ {2,3,4})
print({1,2,3} - {2})
print(sorted({1,2,3} | {3,4,5}))
print(frozenset([1,2,2,3]))
print({1: "a"}.setdefault(2, "b"))
d2 = {"x": 1}
d2.update(y=2)
print(sorted(d2.items()))
print(len({}.keys()))
