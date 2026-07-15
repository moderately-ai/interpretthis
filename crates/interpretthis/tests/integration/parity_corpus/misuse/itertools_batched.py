from itertools import batched, pairwise, count, islice
print(list(batched([1, 2, 3, 4, 5], 2)))
print(list(batched("abcdefg", 3)))
print(list(pairwise([1, 2, 3, 4])))
print(list(pairwise("abc")))
print(list(islice(count(), 5)))
from itertools import filterfalse, compress, dropwhile
print(list(filterfalse(lambda x: x % 2, range(10))))
print(list(compress("ABCDEF", [1, 0, 1, 0, 1, 1])))
print(list(dropwhile(lambda x: x < 5, [1, 3, 6, 2, 1])))
from itertools import chain
print(list(chain.from_iterable([[1, 2], [3], [4, 5]])))
