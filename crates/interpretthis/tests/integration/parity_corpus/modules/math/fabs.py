# math.fabs always returns a float (even for int input) and never preserves
# negative zero.
import math

print(math.fabs(-5))
print(math.fabs(0))
