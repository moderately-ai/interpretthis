# Pin: floats are hashable and usable as dict keys, retrievable by equal float.
# Expected stdout: `x`.
d = {1.5: "x"}
print(d[1.5])
