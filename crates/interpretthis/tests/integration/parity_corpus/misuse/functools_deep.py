import functools
print(functools.reduce(lambda a, b: a + b, [1, 2, 3, 4]))
print(functools.reduce(lambda a, b: a + b, [1, 2, 3], 100))
@functools.lru_cache(maxsize=None)
def fib(n):
    return n if n < 2 else fib(n - 1) + fib(n - 2)
print(fib(20))
print(functools.reduce(lambda a, b: a * b, range(1, 6)))
key = functools.cmp_to_key(lambda a, b: a - b)
print(sorted([3, 1, 2], key=key))
