# Pins: `filter(d.get, keys)` — bound method as the filter predicate.
# A key's truthiness is the value returned by d.get(key).
d = {'A': 1, 'B': 0, 'C': 2}
print(list(filter(d.get, ['A', 'B', 'C'])))
