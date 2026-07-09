# Counter.__missing__ returns 0 WITHOUT inserting an entry — the
# subtle distinction from `dict.get(key, 0)` that user code often
# relies on. Pins types::counter_missing slot and the dict_get_item
# missing_slot dispatch path.
import collections
c = collections.Counter('aabb')
print(c['a'])           # 2
print(c['missing'])     # 0 from __missing__, but no insert
print('missing' in c)   # False — the read above did not materialise an entry
print(len(c))           # 2 still
print(list(c.keys()))   # ['a', 'b']
