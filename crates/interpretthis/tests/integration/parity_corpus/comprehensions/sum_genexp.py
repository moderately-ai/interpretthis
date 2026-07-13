# Pin: `sum(<genexp>)` consumes a generator expression, not just a list.
# Expected stdout: `6`.
print(sum(x for x in [1, 2, 3]))
