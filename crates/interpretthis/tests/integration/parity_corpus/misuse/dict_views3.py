d = {"a": 1, "b": 2, "c": 3}
print(len(d.keys()))
print(list(d.values()))
print(sorted(d.items()))
print("a" in d.keys())
print(1 in d.values())
print(("a", 1) in d.items())
for k in d:
    pass
print(list(d))
print(list(reversed(d)))
print({k: v for k, v in d.items() if v > 1})
d2 = dict.fromkeys(["x", "y", "z"])
print(d2)
d3 = dict.fromkeys(range(3), [])
print(d3)
print(dict(a=1, **{"b": 2}))
merged = {**d, "d": 4}
print(len(merged))
