# Generator output is iterable by every builtin that takes an iterable:
# for / list / tuple / sum / any / all / min / max / sorted. Pins
# Track C's "generator returns a list-wrapped buffer" shape against
# the iterator-consuming builtins.
def fib(n):
    a, b = 0, 1
    for _ in range(n):
        yield a
        a, b = b, a + b

g_list = list(fib(8))
print(g_list)
print(sum(fib(10)))
print(any(x == 5 for x in fib(10)))     # True (5 is in the fibs)
print(all(x >= 0 for x in fib(10)))     # All non-negative
print(min(fib(10)))                     # 0
print(max(fib(10)))                     # 21
print(sorted(fib(8), reverse=True))     # reverse-sorted

# Tuple unpacking from generator
def yield_pair():
    yield 1
    yield 2

a, b = yield_pair()
print(a, b)

# Nested generator in a generator expression
def evens():
    for i in range(10):
        if i % 2 == 0:
            yield i

print([x * x for x in evens()])
