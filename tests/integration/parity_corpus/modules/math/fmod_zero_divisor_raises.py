# fmod with a zero divisor raises ValueError (math domain error).
import math

print(math.fmod(5, 0))
