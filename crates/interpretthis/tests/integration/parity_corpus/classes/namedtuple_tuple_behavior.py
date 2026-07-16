from collections import namedtuple

# A namedtuple subclasses tuple: indexing, slicing, concatenation, repetition,
# membership, iteration, len, unpacking, and count/index all behave like tuple.
P = namedtuple("P", "x y z")
p = P(1, 2, 3)
print(p[0], p[1], p[2])
print(p[-1])
print(p[:2])
print(p[::-1])
print(list(p))
print(tuple(p))
print(len(p))
a, b, c = p
print(a, b, c)
print(p + (4, 5))
print(p * 2)
print((0,) + p)
print(1 in p, 9 in p)
print(p.count(1))
print(p.index(2))

# The builtin namedtuple-shaped results index and slice too.
import datetime as dt
print(dt.date(2024, 12, 31).timetuple()[:3])
print(dt.date(2024, 3, 15).isocalendar()[0])
print(dt.date(2024, 3, 15).isocalendar()[1])

from decimal import Decimal as D
print(D("1.23").as_tuple()[0])
print(D("1.23").as_tuple()[1])
print(D("1.23").as_tuple()[1][0])
