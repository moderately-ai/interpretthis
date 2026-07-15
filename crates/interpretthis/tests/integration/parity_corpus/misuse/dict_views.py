d = {"a": 1, "b": 2, "c": 3}
keys = d.keys()
d["e"] = 5
print(sorted(keys))
print(list(d.items()))
print("a" in d.keys(), 5 in d.values())
print(d.keys() & {"a", "x"})
print(len(d.values()))
items = dict(sorted(d.items(), key=lambda kv: -kv[1]))
print(items)
print({**d, "f": 6})
