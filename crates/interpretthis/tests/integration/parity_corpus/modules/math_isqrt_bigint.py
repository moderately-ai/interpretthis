# math.isqrt is exact integer square root over arbitrary precision. Regression:
# it ran Newton's method in i64 and overflowed (panicking / going negative) for
# large arguments, and rejected ints beyond i64.
import math

print(math.isqrt(0))
print(math.isqrt(15))
print(math.isqrt(16))
print(math.isqrt(2**63 - 1))     # near i64::MAX — used to overflow
print(math.isqrt(2**200))        # beyond i64
print(math.isqrt(10**50))

try:
    math.isqrt(-1)
except ValueError:
    print("ValueError")
try:
    math.isqrt(2.5)
except TypeError:
    print("TypeError")
