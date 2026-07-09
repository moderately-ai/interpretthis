# Pins: dict.update(**kwargs) and OrderedDict.move_to_end(key=, last=).
# dict.get/setdefault are positional-only in CPython 3.12.
d = {"x": 1, "y": 2}
d.update(z=3, y=9)
print(d)
from collections import OrderedDict

od = OrderedDict([("a", 1), ("b", 2), ("c", 3)])
od.move_to_end(key="b", last=False)
print(list(od.keys()))
od.move_to_end(key="b", last=True)
print(list(od.keys()))
