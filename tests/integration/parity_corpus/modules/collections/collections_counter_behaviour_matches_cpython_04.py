# Counting list elements through `collections.Counter` and reading back the
# count for a single key matches CPython.
import collections
c = collections.Counter(['a', 'b', 'a', 'c', 'b', 'a'])
print(c['a'])
