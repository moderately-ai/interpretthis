# Pins: functools.reduce accepts a bound method as the reducer.
# Same-family-as-Bug-1 indirection: reduce's inline match only handles
# Function/Lambda, falling through for BoundMethod with a custom
# TypeError. CPython treats `d.get` like any other callable.
import functools
d = {1: 10, 2: 20}
print(functools.reduce(d.get, [1, 2]))
