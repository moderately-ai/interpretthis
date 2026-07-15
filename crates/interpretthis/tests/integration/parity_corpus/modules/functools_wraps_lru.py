import functools
@functools.lru_cache(maxsize=None)
def fib(n):
    return n if n < 2 else fib(n - 1) + fib(n - 2)
print(fib(10))
print(fib.__name__, fib.__qualname__)
print(hasattr(fib, "__wrapped__"), fib.__wrapped__.__name__)
print(fib.__doc__)

@functools.cache
def add(a, b):
    "add docstring"
    return a + b
print(add(1, 2), add.__name__, add.__doc__)

@functools.wraps(fib)
def rebound():
    return "x"
print(rebound.__name__)

# map over an lru-cached function's __name__ via attribute
print([f.__name__ for f in [fib, add]])
