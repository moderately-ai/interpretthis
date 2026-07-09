# Pin: nested subscript assignment (`d[k1][k2] = v`) mutates the inner dict.
# Expected stdout: `{'a': {'x': 1}}`.
d = {"a": {"x": 0}}
d["a"]["x"] = 1
print(d)
