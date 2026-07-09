# Pins: set union/intersection/difference/symmetric-difference via
# operators. sorted() called to make output deterministic; raw set
# iteration order is unspecified across CPython implementations.
a = {1, 2, 3}
b = {3, 4, 5}
print(sorted(a | b))
print(sorted(a & b))
print(sorted(a - b))
print(sorted(a ^ b))
