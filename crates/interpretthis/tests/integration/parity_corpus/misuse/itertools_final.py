from itertools import combinations_with_replacement, count, islice, accumulate
print(list(combinations_with_replacement([1, 2, 3], 2)))
print(list(islice(count(10), 3)))
print(list(islice(count(0, 5), 4)))
print(list(accumulate([1, 2, 3, 4], initial=100)))
from itertools import product
print(list(product([0, 1], repeat=2)))
print(list(product("ab", "cd")))
from itertools import permutations
print(list(permutations([1, 2, 3])))
print(list(permutations([1, 2, 3], 2)))
from itertools import zip_longest
print(list(zip_longest([1, 2, 3], ["a", "b"], fillvalue="?")))
from itertools import starmap
print(list(starmap(pow, [(2, 3), (2, 4)])))
