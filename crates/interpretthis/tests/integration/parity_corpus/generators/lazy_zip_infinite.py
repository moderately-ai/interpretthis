# zip over an infinite producer must be lazy — including the all-infinite case
# zip(count(), count()), which previously hung — so a downstream islice/next/for
# bounds it. Finite and count+finite cases keep their exact values.
import itertools
c = itertools.count
print(list(itertools.islice(zip(c(), c(10)), 3)))
print(list(itertools.islice(zip(c(), c(10), c(100)), 2)))
print(list(zip(c(), "abc")))
print(list(zip("xy", c(5))))
print(dict(zip("abc", c())))
print([a + b for a, b in zip(c(1), [10, 20, 30])])
z = zip(c(), c())
print(next(z), next(z), next(z))
# lazy zip feeding another lazy consumer
print(list(itertools.islice(itertools.starmap(lambda a, b: a * b, zip(c(1), c(1))), 4)))
# finite / non-lazy unchanged
print(list(zip([1, 2, 3], [4, 5, 6])))
print(list(zip([1, 2], [3, 4, 5])))
print(list(zip("abc", [1, 2, 3], [True, False, True])))
