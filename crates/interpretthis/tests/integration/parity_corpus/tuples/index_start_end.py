# list.index / tuple.index honour their optional start/stop arguments, and
# count takes exactly one argument. Regression: index silently ignored
# start/stop (returning the first match), and count ignored extra args.
xs = [1, 2, 3, 1, 2]
print(xs.index(2, 2))         # first 2 at/after slot 2 -> 4
print(xs.index(1, 1, 4))      # 1 within [1,4) -> 3
print(xs.index(1, -2))        # negative start -> 3
ts = (1, 2, 3, 1, 2)
print(ts.index(3, 0, 5))
print(ts.index(2, 2))

# count has no start/stop; it takes exactly one argument.
print(xs.count(1))
print(ts.count(2))

# Not found within the window raises ValueError.
try:
    xs.index(2, 0, 1)
except ValueError:
    print("ValueError")
try:
    ts.index(9)
except ValueError:
    print("ValueError")

# Non-integer bounds raise TypeError (no None, unlike str.find).
try:
    xs.index(2, 1.5)
except TypeError:
    print("TypeError")
try:
    xs.index(2, None)
except TypeError:
    print("TypeError")

# Extra count arguments raise TypeError.
try:
    xs.count(1, 0)
except TypeError:
    print("TypeError")
