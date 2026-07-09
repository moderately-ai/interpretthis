# combinations(iter, r) returns all r-length sub-tuples in lex order.
import itertools
print(list(itertools.combinations([1, 2, 3, 4], 2)))
print(list(itertools.combinations("abc", 2)))
print(list(itertools.combinations("abcd", 3)))
# r=0 -> one empty tuple
print(list(itertools.combinations([1, 2], 0)))
# r>len -> empty
print(list(itertools.combinations([1, 2], 5)))
