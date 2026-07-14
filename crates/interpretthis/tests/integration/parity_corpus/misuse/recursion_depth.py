def fib(n):
    return n if n < 2 else fib(n - 1) + fib(n - 2)
print(fib(20))
def fact(n):
    return 1 if n <= 1 else n * fact(n - 1)
print(fact(100))
def depth(n):
    if n <= 0:
        return 0
    return 1 + depth(n - 1)
print(depth(500))
import functools
@functools.lru_cache(maxsize=None)
def lfib(n):
    return n if n < 2 else lfib(n - 1) + lfib(n - 2)
print(lfib(30))
