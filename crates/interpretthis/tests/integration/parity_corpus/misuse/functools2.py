from functools import reduce
print(reduce(lambda a, b: a + b, [1, 2, 3, 4]))
print(reduce(lambda a, b: a + b, [1, 2, 3], 100))
from functools import lru_cache
@lru_cache(maxsize=None)
def fib(n):
    return n if n < 2 else fib(n-1) + fib(n-2)
print(fib(10))
from functools import cmp_to_key
print(sorted([3, 1, 2], key=cmp_to_key(lambda a, b: a - b)))
