# Containers are reference-semantic (Arc<Mutex<..>>), and the mutex is
# non-reentrant, so any self-comparison or self-argument method must not lock
# the one mutex twice. Each line here would hang before the deadlock fixes.
ba = bytearray(b"abc")
print(ba == ba)

l = [3, 1, 2]
print(l == l, l < l, l <= l, l > l, l >= l)

from dataclasses import dataclass


@dataclass
class P:
    x: int
    y: int


p = P(1, 2)
print(p == p)

s = {1, 2, 3}
print(s == s, s is s)
print(sorted(s.union(s)), sorted(s.intersection(s)), sorted(s.difference(s)))
print(s.issubset(s), s.issuperset(s), s.isdisjoint(s))
print(sorted(s.symmetric_difference(s)))
s.update(s)
print(sorted(s))
s.intersection_update(s)
print(sorted(s))
s.difference_update(s)
print(sorted(s))

s2 = {4, 5, 6}
s2.symmetric_difference_update(s2)
print(sorted(s2))

fs = frozenset({7, 8, 9})
print(fs == fs, sorted(fs.union(fs)), fs.issubset(fs))

lst = [1, 2, 3]
lst.extend(lst)
print(lst)

d = {"a": 1, "b": 2}
d.update(d)
print(sorted(d.items()))
