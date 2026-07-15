# Across the numeric tower, equal values that hash equally collapse to one
# element, and the FIRST inserted keeps both the value and the slot. Order,
# dedup, and repr must all match CPython.
print({1, 1.0, True, 2})
print({True, 1, 1.0})
print({1.0, 1, True})
print(list({1, 1.0, True, 2, 3, 0, False}))
print({0, False, 0.0})
print(len({1, 1.0, True}))
print(1 in {1.0}, True in {1}, 1.0 in {True})

# Complex and Fraction/Decimal participate too.
from fractions import Fraction
from decimal import Decimal

print(2 in {Fraction(2, 1)}, Decimal(3) in {3})
print(list({Fraction(1, 2), 0.5}))
print({1 + 0j, 1})

# Mixed set from a range and a generator, order preserved by the table.
print(list({x % 5 for x in range(20)}))
print(set(range(8)) | {10, 11})
