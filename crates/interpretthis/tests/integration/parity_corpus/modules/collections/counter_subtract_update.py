# Counter.subtract / Counter.update mutate in place. update INCREMENTS
# counts (unlike dict.update which overwrites); subtract decrements
# and can produce zero / negative counts (unlike + / - which
# keep_positive). Pins counter_apply_in_place + dispatch_counter_method.
import collections
c = collections.Counter('aabb')
c.update(['a', 'a', 'c'])
print(sorted(c.items()))        # [('a', 4), ('b', 2), ('c', 1)]
c.subtract({'a': 5, 'b': 1})
print(sorted(c.items()))        # [('a', -1), ('b', 1), ('c', 1)]
# Empty update is a no-op.
c2 = collections.Counter('xy')
c2.update()
print(sorted(c2.items()))       # [('x', 1), ('y', 1)]
# isinstance(Counter, dict) is True because Counter is a dict
# subclass per CPython.
print(isinstance(c, dict))
print(isinstance(c, collections.Counter))
