# math.fsum: exact floating-point summation with CPython's overflow/special
# handling.
import math

def show(f):
    try:
        print(repr(f()))
    except (OverflowError, ValueError, TypeError) as e:
        print(type(e).__name__, e)

print(math.fsum([0.1] * 10))
print(math.fsum([]))
print(math.fsum([1, 2, 3, 4, 5]))
print(math.fsum(range(1, 101)))
print(math.fsum([1e308, -1e308]))
print(math.fsum([1.0, float("inf")]))
print(math.fsum([float("nan"), 1.0]))
show(lambda: math.fsum([1e308, 1e308]))
show(lambda: math.fsum([1e308, 1e308, -1e308, -1e308]))
show(lambda: math.fsum([float("inf"), float("-inf")]))
show(lambda: math.fsum([1, "x"]))
