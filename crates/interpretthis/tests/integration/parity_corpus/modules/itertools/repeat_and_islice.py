# repeat(obj, times) — bounded form. islice for slicing iterators.
import itertools
print(list(itertools.repeat("x", 4)))
print(list(itertools.repeat(0, 0)))
# islice(iter, stop)
print(list(itertools.islice([1, 2, 3, 4, 5], 3)))
# islice(iter, start, stop)
print(list(itertools.islice([1, 2, 3, 4, 5], 1, 4)))
# islice(iter, start, stop, step)
print(list(itertools.islice([1, 2, 3, 4, 5, 6, 7, 8], 0, 8, 2)))
