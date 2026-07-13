# Pins: itertools.product / permutations / combinations / chain /
# islice — common combinatoric building blocks.
#
# itertools.count is intentionally not used here: this interpreter
# does not model lazy infinite iterators, so unbounded count would
# materialise forever. Use range() for bounded sequences instead.
from itertools import product, permutations, combinations, chain, islice

print(list(product([1, 2], "ab")))
print(list(permutations([1, 2, 3], 2)))
print(list(combinations("abcd", 2)))
print(list(chain([1, 2], [3, 4], [5])))
print(list(islice(range(10, 100), 5)))
