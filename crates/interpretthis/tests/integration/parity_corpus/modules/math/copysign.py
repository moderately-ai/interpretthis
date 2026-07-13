# math.copysign preserves the sign of the second argument, including the sign
# of negative zero (so copysign(0, -1) is -0.0).
import math

print(math.copysign(3, -1))
print(math.copysign(-3, 1))
print(math.copysign(0, -1))
