a = {"x": 1}
b = {"y": 2}
print(a | b)
c = {"x": 1}
c |= {"z": 3}
print(c)
print(dict(zip("abc", [1, 2, 3])))
d = {"a": 1, "b": 2}
print(list(d.keys()), list(d.values()))
print({k: v for k, v in d.items() if v > 1})
