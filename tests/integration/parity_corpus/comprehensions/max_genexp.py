# Pin: `max(<genexp>)` consumes a generator expression.
# Expected stdout: `3`.
print(max(x for x in [1, 2, 3]))
