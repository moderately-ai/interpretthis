# math.gcd is variadic and arbitrary-precision. Regression: it read only the
# first two arguments (so gcd(12, 18, 5) gave 6 instead of 1) and rejected ints
# beyond i64.
import math

print(math.gcd(12, 18, 5))       # 1 — third arg matters
print(math.gcd(12, 18, 24))      # 6
print(math.gcd())                # 0
print(math.gcd(48))              # 48 (one arg -> abs)
print(math.gcd(-12, -18))        # 6 (magnitude)
print(math.gcd(2**70, 2**68))    # beyond i64
print(math.gcd(0, 0))            # 0

try:
    math.gcd(12, 2.5)
except TypeError:
    print("TypeError")
