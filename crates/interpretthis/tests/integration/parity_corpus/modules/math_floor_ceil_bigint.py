# math.floor/ceil/trunc return the exact integer, promoting past i64 rather than
# saturating. Regression: `as i64` clamped 1e30 to i64::MAX.
import math

print(math.floor(2.7))
print(math.ceil(2.1))
print(math.trunc(-2.7))
print(math.floor(1e30))
print(math.ceil(1e30))
print(math.trunc(-1e30))

try:
    math.floor(float("inf"))
except OverflowError:
    print("OverflowError")
try:
    math.floor(float("nan"))
except ValueError:
    print("ValueError")
