# map / filter / enumerate over an infinite producer are lazy iterators (as in
# CPython), so a downstream islice / next bounds them instead of hanging. Finite
# and multi-iterable inputs keep their exact values.
import itertools
c = itertools.count
print(list(itertools.islice(map(lambda x: x * 2, c()), 4)))
print(list(itertools.islice(filter(lambda x: x % 2 == 0, c()), 4)))
print(list(itertools.islice(map(lambda a, b: a + b, c(), c(10)), 3)))
print(list(itertools.islice(filter(None, c()), 3)))
print(list(itertools.islice(enumerate(c(100)), 3)))
print(list(itertools.islice(enumerate(c(), start=10), 3)))
m = map(str, c())
print(next(m), next(m), next(m))
f = filter(lambda x: x > 5, c())
print(next(f), next(f))
e = enumerate(c())
print(next(e), next(e))
# finite / non-lazy unchanged
print(list(map(lambda x: x + 1, [1, 2, 3])))
print(list(filter(None, [0, 1, 2, 0, 3])))
print(list(map(lambda x, y: x * y, [1, 2, 3], [4, 5, 6])))
print(list(enumerate("abc")))
print(list(enumerate([10, 20], start=1)))
