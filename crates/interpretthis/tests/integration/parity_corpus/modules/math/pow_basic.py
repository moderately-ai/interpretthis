# math.pow always returns a float (distinct from the ** operator which returns
# an int for integer operands).
import math

print(math.pow(2, 10))
print(math.pow(-2, 3))
