# Pins: math.isnan / .isinf detect special floats; NaN never
# compares equal to itself; math.inf compares as larger than any
# finite number.
import math
print(math.isnan(float('nan')))
print(math.isinf(float('inf')))
print(math.inf > 1000)
print(float('nan') != float('nan'))
