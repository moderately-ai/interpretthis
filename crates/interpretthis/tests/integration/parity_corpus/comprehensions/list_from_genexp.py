# Pin: `list(<genexp>)` materialises the generator into a list.
# Expected stdout: `[1, 2, 3]`.
print(list(x for x in [1, 2, 3]))
