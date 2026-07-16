import itertools
def counter():
    n = 0
    while True:
        n += 1
        yield n
# lazy consumption of genexp over infinite generator
def take(gen, n):
    return [next(gen) for _ in range(n)]
print(take((x*x for x in counter()), 4))
print(list(itertools.islice((x+1 for x in counter()), 5)))
print(next(x*2 for x in counter()))
g = (x for x in counter())
print(next(g), next(g), next(g))
# genexp over infinite itertools source
print(list(itertools.islice((y*y for y in itertools.count(1)), 4)))
# eager consumption (sync consumers) over finite sources
print(", ".join(str(x) for x in range(4)))
print(", ".join(str(x) for x in (i*i for i in range(4))))
def squares(n):
    for i in range(n): yield i*i
print(", ".join(str(x) for x in squares(4)))
print(sum(x for x in squares(5)))
print(dict((i, i*i) for i in range(3)))
print(sorted(x for x in squares(5)))
print(max(x for x in squares(5)))
print(set(x % 3 for x in range(10)))
print(tuple(x for x in squares(3)))
a, b, c = (x for x in range(3))
print(a, b, c)
print(list(x for x in range(5) if x % 2 == 0))
print([f() for f in (lambda: k for k in range(3))])
print(any(x > 100 for x in squares(20)))
