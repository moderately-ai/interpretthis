def gen():
    yield 1
    yield 2
    yield 3
print(list(gen()))
print(sum(gen()))
g = gen()
print(next(g), next(g))
def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b
import itertools
print(list(itertools.islice(fib(), 10)))
def squares(n):
    for i in range(n):
        yield i * i
print([x for x in squares(5)])
print(tuple(squares(4)))
def countdown(n):
    while n > 0:
        yield n
        n -= 1
print(list(countdown(3)))
print(max(squares(5)), min(squares(5)))
