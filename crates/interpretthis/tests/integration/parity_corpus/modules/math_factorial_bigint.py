# math.factorial returns an arbitrary-precision integer, not an i64 that
# overflows at 21! — regression for the bigint accumulation.
import math
print(math.factorial(0), math.factorial(1), math.factorial(5))
print(math.factorial(20), math.factorial(21), math.factorial(30))
print(math.factorial(50))
print(math.factorial(100))
print(math.factorial(True), math.factorial(False))
try:
    math.factorial(-1)
except ValueError as e:
    print("neg:", e)
try:
    math.factorial(2.5)
except TypeError as e:
    print("float:", e)
print(math.factorial(13) == math.perm(13))
print(math.factorial(25) // math.factorial(23))
