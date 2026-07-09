# Pin: `all(<genexp>)` returns `True` when every yielded value is truthy.
# Expected stdout: `True`.
print(all(x > 0 for x in [1, 2, 3]))
