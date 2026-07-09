# Indexing a present key on `collections.Counter` matches CPython for both
# the value type (int) and the printed form.
import collections
c = collections.Counter('aabbc')
print(c['a'])
print(c['b'])
