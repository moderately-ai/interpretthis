# isqrt rejects float input with TypeError, even when the value is an exact
# integer represented as a float.
import math

print(math.isqrt(4.0))
