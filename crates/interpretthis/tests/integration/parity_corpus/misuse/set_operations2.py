a = {1, 2, 3, 4}
b = {3, 4, 5, 6}
print(sorted(a | b))
print(sorted(a & b))
print(sorted(a - b))
print(sorted(a ^ b))
print(a.issubset({1, 2, 3, 4, 5}))
print(a.issuperset({1, 2}))
print(a.isdisjoint({7, 8}))
a.symmetric_difference_update(b)
print(sorted(a))
c = {1, 2, 3}
c.discard(2)
c.discard(99)
print(sorted(c))
print(frozenset([1, 2]) | frozenset([2, 3]))
print(len({1, 2, 2, 3, 3, 3}))
