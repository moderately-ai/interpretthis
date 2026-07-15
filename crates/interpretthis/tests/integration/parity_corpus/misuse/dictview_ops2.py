d = {"a": 1, "b": 2, "c": 3}
keys = d.keys()
print("a" in keys)
print(sorted(keys & {"a", "b", "z"}))
print(sorted(keys | {"d"}))
print(sorted(keys - {"a"}))
items = d.items()
print(("a", 1) in items)
values = d.values()
print(2 in values)
print(len(keys), len(values), len(items))
d["e"] = 5
print("e" in keys)
print(list(sorted(d.keys())))
e = {"a": 1, "b": 2}
print(e.keys() == {"a", "b"})
print(e.items() <= {("a", 1), ("b", 2), ("c", 3)})
