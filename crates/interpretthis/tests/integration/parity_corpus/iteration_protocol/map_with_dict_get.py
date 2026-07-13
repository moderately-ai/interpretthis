# Pins: `map(d.get, keys)` — bound method as the map function. Customer-listed pattern.
d = {'A': 1, 'B': 2, 'C': 3}
print(list(map(d.get, ['A', 'B', 'C'])))
