# callable.__call__(args) is the explicit form of callable(args), for every
# first-class callable — a bare-name (place) receiver and an attribute receiver.
def f(a, b, c):
    return a + b + c


print(f.__call__(1, 2, 3))

g = lambda x: x * 2
print(g.__call__(5))

print((lambda: "anon").__call__())

import functools


@functools.lru_cache
def cached(x):
    return x + 1


print(cached.__call__(10))

p = functools.partial(f, 1, 2)
print(p.__call__(3))

# Attribute-receiver callables (not a bare-name place).
print(str.upper.__call__("hi"))


class C:
    def m(self):
        return "method"


c = C()
print(c.m.__call__())
