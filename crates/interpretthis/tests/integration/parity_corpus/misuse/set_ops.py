a = {1, 2, 3}
b = {2, 3, 4}
print(sorted(a | b))
print(sorted(a & b))
print(sorted(a - b))
print(sorted(a ^ b))
print(a.issubset({1, 2, 3, 4}))
print(a.isdisjoint({5, 6}))
a.symmetric_difference_update(b)
print(sorted(a))
fs = frozenset([1, 2, 2, 3])
print(len(fs))
print(2 in fs)
