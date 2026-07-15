# Set operations must reproduce CPython's hash-table slot order in the RESULT,
# not merely the right membership. CPython presizes a copy of one operand and
# merges the other in slot order; rebuilding the result by re-inserting a bare
# element list can land elements in different slots after a differently-timed
# resize. Print unsorted so any ordering divergence shows.
a = {1, 9, 17, 25, 3, 11, 19, 27, 5, 13}
b = {2, 9, 18, 25, 4, 11, 20, 27, 6, 13}

print(list(a | b))
print(list(a & b))
print(list(a - b))
print(list(a ^ b))
print(list(a.union(b)))
print(list(a.intersection(b)))
print(list(a.difference(b)))
print(list(a.symmetric_difference(b)))

# Multi-operand and asymmetric sizes exercise the presize/merge order further.
c = {100, 200, 300}
print(list(a | b | c))
print(list(a.union(b, c)))
print(list(a.intersection({9, 11, 13, 25, 27, 99})))

# Mutating operations mutate the receiver's table in place; order after update.
d = {1, 9, 17, 25, 3}
d.update({2, 9, 18, 25, 4, 100})
print(list(d))
d.intersection_update({9, 25, 2, 4})
print(list(d))
e = {1, 9, 17, 25, 3, 11}
e.symmetric_difference_update({9, 25, 40, 41})
print(list(e))
e.difference_update({40, 1})
print(list(e))

# Larger set to force multiple resizes during construction and operation.
big_a = set(range(0, 40, 2))
big_b = set(range(0, 40, 3))
print(list(big_a | big_b))
print(list(big_a & big_b))
print(list(big_a - big_b))
print(list(big_a ^ big_b))

# frozenset operations follow the same table rules.
fa = frozenset({1, 9, 17, 25, 3, 11})
fb = frozenset({2, 9, 18, 25, 4, 11})
print(list(fa | fb))
print(list(fa & fb))
print(list(fa ^ fb))
