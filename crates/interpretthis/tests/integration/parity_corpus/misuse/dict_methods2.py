d = {"a": 1}
print(d.setdefault("a", 99))
print(d.setdefault("b", 2))
print(d)
d.update({"c": 3}, d=4)
print(sorted(d.items()))
print(d.get("z", "default"))
print(dict.fromkeys(["x", "y"], 0))
e = {"a": 1, "b": 2}
e |= {"c": 3}
print(sorted(e.items()))
print({**{"a": 1}, **{"b": 2}})
