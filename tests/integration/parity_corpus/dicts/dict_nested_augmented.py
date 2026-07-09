# Pin: augmented assignment (`d[k1][k2] += 1`) reads-modifies-writes the inner value.
# Expected stdout: `{'a': {'x': 2}}`.
d = {"a": {"x": 1}}
d["a"]["x"] += 1
print(d)
