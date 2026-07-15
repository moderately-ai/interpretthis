d1 = {"a": 1, "b": 2, "c": 3}
d2 = {"b": 20, "c": 30, "d": 40}
print(d1.keys() & d2.keys(), d1.keys() | d2.keys())
print(d1.keys() - d2.keys(), d1.keys() ^ d2.keys())
print(d1.items() & {("b", 2), ("x", 9)})
print(list(d1.keys()), list(d1.values()), list(d1.items()))
print("a" in d1.keys(), 1 in d1.values(), ("a", 1) in d1.items())
print(len(d1.keys()), len(d1.values()))
dv = d1.keys()
d1["e"] = 5
print("e" in dv, len(dv))
print(dict(zip(d1.keys(), d1.values())) == d1)
print(sorted(d1.keys() | d2.keys()))
