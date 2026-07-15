def gen():
    yield 1
    yield 2
    return 99
g = gen()
print(next(g), next(g))
try:
    next(g)
except StopIteration as e:
    print("stop", e.value)
def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a+b
import itertools
print(list(itertools.islice(fib(), 8)))
print(list(x*x for x in range(5)))
def countdown(n):
    while n > 0:
        yield n
        n -= 1
print(list(countdown(4)))
gen2 = (i for i in range(3))
print(next(gen2))
print(list(gen2))
