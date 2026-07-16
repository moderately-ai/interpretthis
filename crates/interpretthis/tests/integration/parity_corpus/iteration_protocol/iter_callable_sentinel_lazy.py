# iter(callable, sentinel) is lazy: it calls the callable once per next() and
# stops when the result equals the sentinel — so an unbounded form streams
# rather than materialising.
print(next(iter(int, 1)))

# Partial consumption of an unbounded callable_iterator.
c = [0]


def inc():
    c[0] += 1
    return c[0]


it = iter(inc, 100)
print(next(it), next(it), next(it))

# islice bounds an otherwise-infinite iter(int, 1).
from itertools import islice

print(list(islice(iter(int, 1), 4)))

# Finite: the callable eventually returns the sentinel.
data = [1, 2, 3, 0, 4]
idx = [0]


def read():
    v = data[idx[0]]
    idx[0] += 1
    return v


print(list(iter(read, 0)))

# Immediate sentinel yields nothing.
print(list(iter(lambda: "stop", "stop")))

# for-loop consumption stops at the sentinel.
vals = iter([10, 20, 30, 99])
print([x for x in iter(lambda: next(vals), 99)])
