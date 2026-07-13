# Counter.most_common([n]) returns the top-n (key, count) pairs
# sorted by count desc, with stable order for ties (insertion order
# as the tie-breaker, matching CPython).
import collections
c = collections.Counter('abracadabra')
print(c.most_common())          # all entries
print(c.most_common(2))         # top 2
print(c.most_common(1))         # top 1
print(c.most_common(0))         # empty
# Empty Counter:
print(collections.Counter().most_common())
