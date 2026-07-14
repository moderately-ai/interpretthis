d = {"b": 2, "a": 1, "c": 3}
print(list(d.keys()), list(d.values()))
print(sorted(d.items()))
d["d"] = 4
del d["b"]
print(list(d))
print({k: v * 2 for k, v in d.items()})
print(max(d, key=d.get), min(d, key=d.get))
merged = {**d, "e": 5}
print(len(merged))
