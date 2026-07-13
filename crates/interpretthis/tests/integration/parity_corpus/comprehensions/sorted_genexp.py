# Pin: `sorted(<genexp>)` materialises the generator into a sorted list.
# Expected stdout: `[1, 2, 3]`.
print(sorted(x for x in [3, 1, 2]))
