d = {"a": 1, "b": 2}
print(type(d.keys()).__name__, type(d.values()).__name__, type(d.items()).__name__)
print(sorted(d.keys() | {"c"}))
print(sorted(d.items() - {("a", 1)}))
print(dict(d.items()))
print(len(d.items()) == 2)
print(d.keys() == {"a", "b"})
print("a" in d.keys(), ("b", 2) in d.items(), 1 in d.values())
