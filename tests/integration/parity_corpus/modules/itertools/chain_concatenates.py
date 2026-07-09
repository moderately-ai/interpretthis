# itertools.chain(*iterables) concatenates iterables left-to-right.
import itertools
print(list(itertools.chain([1, 2], [3, 4])))
print(list(itertools.chain([], [1, 2], [], [3])))
print(list(itertools.chain("ab", "cd")))
print(list(itertools.chain(range(3), [10, 20])))
print(list(itertools.chain()))
