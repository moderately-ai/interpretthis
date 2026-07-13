# math.isfinite is true only for finite values; both inf and nan return False.
import math

print(math.isfinite(1.0))
print(math.isfinite(float('inf')))
print(math.isfinite(float('nan')))
