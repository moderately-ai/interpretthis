def gen():
    x = yield 1
    y = yield x + 1
    yield y * 2
g = gen()
print(next(g))
print(g.send(10))
print(g.send(5))
def fib_gen():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b
import itertools
print(list(itertools.islice(fib_gen(), 8)))
print([x*x for x in (i for i in range(5))])
print(sum(x for x in range(10)))
gen_exp = (x for x in range(3))
print(next(gen_exp), list(gen_exp))
