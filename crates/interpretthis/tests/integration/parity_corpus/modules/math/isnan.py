# math.isnan distinguishes NaN from any finite value.
import math

print(math.isnan(float('nan')))
print(math.isnan(1.0))
