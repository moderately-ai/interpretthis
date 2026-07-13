# permutations + product cover the canonical itertools idioms.
import itertools
print(list(itertools.permutations([1, 2, 3])))
print(list(itertools.permutations("abc", 2)))
# product: Cartesian
print(list(itertools.product([1, 2], [3, 4])))
print(list(itertools.product("ab", "cd")))
print(list(itertools.product()))
# Empty pool -> empty product
print(list(itertools.product([1, 2], [])))
