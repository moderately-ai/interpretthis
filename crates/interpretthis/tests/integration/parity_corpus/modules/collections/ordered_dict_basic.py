# OrderedDict — thin shim returning a plain Dict (which is already
# insertion-ordered in CPython 3.7+). Print via list(items()) to avoid
# the OrderedDict/dict repr difference (CPython prints
# `OrderedDict({...})`, our shim returns a plain dict — a deliberate
# simplification per the OrderedDict-as-thin-shim rationale).
import collections
d = collections.OrderedDict([("a", 1), ("b", 2), ("c", 3)])
print(list(d.items()))
print(list(d.keys()))
print(list(d.values()))
# Empty
print(list(collections.OrderedDict().items()))
# From dict
print(list(collections.OrderedDict({"x": 1, "y": 2}).items()))
