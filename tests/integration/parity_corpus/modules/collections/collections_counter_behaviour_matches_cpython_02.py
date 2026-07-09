# Distinct-element count via `len(Counter)` and total count via
# `sum(Counter.values())` match CPython's behaviour.
import collections
c = collections.Counter([1, 2, 1, 3, 2, 1])
print(len(c))
print(sum(c.values()))
