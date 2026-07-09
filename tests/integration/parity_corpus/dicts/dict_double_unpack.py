# Pins: dict double-unpacking `{**a, **b, ...}` produces a merged
# dict in declaration order; later values overwrite earlier ones.
base = {'a': 1, 'b': 2}
extra = {'c': 3}
combined = {**base, **extra, 'd': 4}
print(combined)
