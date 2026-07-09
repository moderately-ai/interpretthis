# Pins: namedtuple is iterable like a tuple (field order).
from collections import namedtuple

Point = namedtuple("Point", "x y")
p = Point(3, 4)
print(list(p))
print(len(p))
for v in p:
    print(v)
a, b = p
print(a, b)
print(sum(p))
