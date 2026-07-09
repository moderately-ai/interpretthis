# log10(0) raises ValueError (math domain error) just like log(0).
import math

print(math.log10(0))
