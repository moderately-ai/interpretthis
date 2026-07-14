def gen():
    try:
        yield 1
        yield 2
    except ValueError:
        yield 99
g = gen()
print(next(g))
print(g.throw(ValueError))
def counter():
    i = 0
    while True:
        i += 1
        yield i
c = counter()
print(next(c), next(c), next(c))
