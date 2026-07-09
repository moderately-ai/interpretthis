# Pins: `min(d, key=d.get)` — bound method as key function. Mirrors the
# customer's `max(d, key=d.get)` reproducer on the `min` side.
d = {'A': 1, 'B': 2, 'C': 3}
print(min(d, key=d.get))
