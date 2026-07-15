import functools
def f(a, b, c):
    return (a, b, c)
p = functools.partial(f, 1, 2)
print(p(3))
p2 = functools.partial(f, 1, c=10)
print(p2(2))
add = functools.partial(lambda x, y: x + y, 10)
print(add(5))
print(list(map(functools.partial(pow, 2), [1, 2, 3])))
r = functools.reduce(lambda a, b: a * b, range(1, 5), 1)
print(r)
