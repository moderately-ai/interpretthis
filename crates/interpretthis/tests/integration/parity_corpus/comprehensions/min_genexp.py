# Pin: `min(<genexp>)` consumes a generator expression.
# Expected stdout: `1`.
print(min(x for x in [1, 2, 3]))
