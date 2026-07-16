# Dict views are reversible (CPython 3.8+): reversed() over keys/values/items
# yields insertion order backwards, with the correctly-named reverse iterator.
d = {"a": 1, "b": 2, "c": 3}
print(list(reversed(d.keys())))
print(list(reversed(d.values())))
print(list(reversed(d.items())))
print(type(reversed(d.keys())).__name__)
print(type(reversed(d.values())).__name__)
print(type(reversed(d.items())).__name__)
print([k for k in reversed(d.keys())])
print(dict(reversed(d.items())))
print(sorted(reversed(d.values())))

# A view reversed after the dict grows reflects the live contents.
e = {1: "x", 2: "y"}
e[3] = "z"
print(list(reversed(e.keys())))
