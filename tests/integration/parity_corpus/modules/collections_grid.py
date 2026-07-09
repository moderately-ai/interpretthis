# Pins: OrderedDict + namedtuple — common customer collection types.
from collections import OrderedDict, namedtuple

od = OrderedDict([('a', 1), ('b', 2)])
print(list(od.keys()))
print(list(od.values()))

Point = namedtuple('Point', ['x', 'y'])
p = Point(3, 4)
print(p.x, p.y)
print(p[0], p[1])
print(p._asdict())
