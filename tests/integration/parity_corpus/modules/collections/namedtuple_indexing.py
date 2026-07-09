# namedtuple subscript — positional access mirrors field order.
#
# Pins CPython semantics: nt[0] returns the first field, nt[-1] the
# last; out-of-range indices raise.
from collections import namedtuple

Point = namedtuple("Point", "x y")
p = Point(3, 4)

# Subscript access.
print(p[0])
print(p[1])
print(p[-1])
print(p[-2])

# Attribute access still works (does not regress).
print(p.x)
print(p.y)

# _fields tuple is exposed.
print(Point._fields)

# Out-of-range index.
try:
    p[5]
except Exception:
    print("IndexError")
