# Pin: `any(<genexp>)` returns `True` when at least one yielded value is truthy.
# Expected stdout: `True`.
print(any(x > 2 for x in [1, 2, 3]))
