# bool is a subclass of int in CPython, so factorial accepts True/False and
# returns the integer factorial of 1 / 0.
import math

print(math.factorial(True))
print(math.factorial(False))
