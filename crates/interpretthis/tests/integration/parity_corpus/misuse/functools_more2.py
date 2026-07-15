from functools import partial, reduce, lru_cache, cache
def multiply(x, y, z):
    return x * y * z
double = partial(multiply, 2)
print(double(3, 4))
triple_double = partial(multiply, 2, 3)
print(triple_double(4))
print(partial(sorted, reverse=True)([3, 1, 2]))
@lru_cache
def factorial(n):
    return 1 if n <= 1 else n * factorial(n-1)
print(factorial(5))
print(factorial(10))
@cache
def fib(n):
    return n if n < 2 else fib(n-1) + fib(n-2)
print(fib(15))
print(reduce(lambda a, b: a + b, range(1, 11)))
print(reduce(lambda a, b: max(a, b), [3, 7, 2, 8, 1]))
from functools import cmp_to_key
print(sorted([3, 1, 2], key=cmp_to_key(lambda a, b: b - a)))
