a = {1, 2, 3}
a.add(4)
a.update([5, 6], {7, 8})
print(sorted(a))
a.discard(100)
a.discard(1)
print(sorted(a))
print(a.pop() in {2, 3, 4, 5, 6, 7, 8})
b = frozenset([1, 2, 3])
print(b | {4}, type(b | {4}).__name__)
print(b & {2, 3, 4}, b - {1}, b ^ {3, 4})
print({1, 2, 3}.isdisjoint({4, 5}))
print({1, 2}.issubset({1, 2, 3}), {1, 2, 3}.issuperset({1}))
s = {1, 2, 3, 4, 5}
s.intersection_update({2, 3, 4, 9})
print(sorted(s))
s.symmetric_difference_update({3, 10})
print(sorted(s))
s.difference_update({10})
print(sorted(s))
print(set() == frozenset())
print({1, 2, 3} == {3, 2, 1})
print(len({frozenset([1, 2]), frozenset([2, 1]), frozenset([3])}))
print({x for x in range(20) if x % 3 == 0})
print(set("mississippi"))
nested = {frozenset([1, 2]): "a", frozenset([3]): "b"}
print(nested[frozenset([2, 1])])
c = {1, 2, 3}
d = c.copy()
d.add(99)
print(99 in c, 99 in d)
print({1, 2, 3} - {1, 2, 3})
print(frozenset() | frozenset([1]))
print(set(range(5)) & set(range(3, 10)))
print(bool(set()), bool({0}))
x = {1, 2, 3}
x |= {4, 5}
x &= {2, 3, 4}
x -= {2}
x ^= {3, 9}
print(sorted(x))
