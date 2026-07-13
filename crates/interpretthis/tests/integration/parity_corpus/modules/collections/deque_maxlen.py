# deque(iterable, maxlen) caps the queue size — pushes evict from the
# opposite end.
import collections
d = collections.deque([1, 2, 3], 3)
print(d)
d.append(4)             # 1 evicted from front
print(d)
d.appendleft(0)         # 4 evicted from back
print(d)
# maxlen=0 always empty
e = collections.deque([1, 2], 0)
print(e)
print(len(e))
# extend respects maxlen
d2 = collections.deque([], 3)
d2.extend([1, 2, 3, 4, 5])
print(d2)               # [3, 4, 5]
