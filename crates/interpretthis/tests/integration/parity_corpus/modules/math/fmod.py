# math.fmod follows the C semantics (result has the sign of the dividend),
# which differs from Python's % operator (sign of the divisor).
import math

print(math.fmod(10, 3))
print(math.fmod(-10, 3))
print(math.fmod(10, -3))
