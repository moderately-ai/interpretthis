# `<=` / `>=` must use the same equality as `==`. Regression: these routed
# through a second, incomplete equality table that had no arm for BigInt,
# Decimal, or Fraction, so `x <= x` was False for every value outside i64.
from decimal import Decimal
from fractions import Fraction

big = 10**20
print(big <= big)
print(big >= big)
print(big <= big + 1)
print(big + 1 >= big)

d = Decimal("1.5")
print(d <= d)
print(d >= d)
print(d <= Decimal("2.0"))

f = Fraction(1, 3)
print(f <= f)
print(f >= f)
print(Fraction(1, 4) <= f)

# equal-but-distinct across the i64 boundary
print(10**19 <= 10**19)
print((2**63) <= (2**63))
