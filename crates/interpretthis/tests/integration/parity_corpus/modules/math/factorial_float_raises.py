# factorial rejects non-integer floats with TypeError
# ("'float' object cannot be interpreted as an integer"). Even an exact-integer
# float (3.5 here is fractional, but the same rule applies to 3.0).
import math

print(math.factorial(3.5))
