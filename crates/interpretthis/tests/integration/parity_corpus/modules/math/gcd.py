# math.gcd: always non-negative, gcd(0, 0) == 0, accepts negative inputs by
# taking the absolute value.
import math

print(math.gcd(12, 8))
print(math.gcd(0, 5))
print(math.gcd(0, 0))
print(math.gcd(-4, 6))
