import functools
@functools.lru_cache(maxsize=None)
def fib(n): return n if n < 2 else fib(n-1) + fib(n-2)
print(fib(30))
print(functools.reduce(lambda a, b: a + b, [1, 2, 3, 4]))
print(functools.reduce(lambda a, b: a * b, [1, 2, 3, 4], 1))
add = functools.partial(lambda a, b, c: a + b + c, 1, 2)
print(add(3))
@functools.total_ordering
class N:
    def __init__(self, v): self.v = v
    def __eq__(self, o): return self.v == o.v
    def __lt__(self, o): return self.v < o.v
print(N(1) < N(2), N(2) > N(1), N(1) <= N(1), N(2) >= N(3))
print(functools.reduce(lambda a, b: a - b, [10, 1, 2, 3]))
@functools.cache
def sq(x): return x * x
print(sq(5), sq(5))
