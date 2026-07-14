import math
print(math.floor(1e30) == 10**30)
print(math.gcd(12, 18, 24))
try:
    print(math.sqrt(-1))
except ValueError as e:
    print("sqrt:", type(e).__name__)
try:
    print(math.log(8, 0))
except (ValueError, ZeroDivisionError) as e:
    print("log:", type(e).__name__)
print(math.isqrt(2**63 - 1))
