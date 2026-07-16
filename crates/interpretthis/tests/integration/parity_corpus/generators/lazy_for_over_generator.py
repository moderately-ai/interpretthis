# A `for x in <generator>` inside a generator body must step the source ONE item
# at a time, not materialise (drain) it. Otherwise an early break over-consumes
# the source — and hangs on an infinite one — and loop-variable closures capture
# the wrong value. itertools.islice is lazy for the same reason.
import itertools

def inner():
    for k in range(5):
        yield k

def take(src, n):
    i = 0
    for x in src:
        if i >= n:
            break
        yield x
        i += 1

print(list(take(inner(), 3)))
print(list(take(itertools.count(), 4)))          # infinite source, early stop
print(list(take(itertools.count(10, 5), 3)))

# Loop-variable closures pulled interleaved see their own value.
def lazy_lambdas():
    for k in range(100):
        yield lambda: k
print([f() for f in take(lazy_lambdas(), 4)])
print([f() for f in itertools.islice((lambda: k for k in range(100)), 4)])

# map-like / filter-like full iteration stays correct.
def doubler(src):
    for x in src:
        yield x * 2
print(list(doubler(inner())))
print(list(doubler(x for x in range(4))))

def evens(src):
    for x in src:
        if x % 2 == 0:
            yield x
print(list(itertools.islice(evens(itertools.count()), 3)))

# The source is consumed exactly as far as needed (side effects prove it).
log = []
def noisy():
    for k in range(100):
        log.append(k)
        yield k
print(list(take(noisy(), 3)), log)

# islice general forms.
print(list(itertools.islice(range(20), 2, 12, 3)))
print(list(itertools.islice(itertools.count(), 5)))
print(list(itertools.islice(itertools.count(10, 2), 2, 8, 2)))
print(list(itertools.islice((x * x for x in range(10)), 3, 8)))
print(list(itertools.islice(range(10), 3, None)))
print([f(10) for f in itertools.islice((lambda x: x + i for i in range(10)), 2, 6, 2)])
print(list(itertools.islice(itertools.islice(itertools.count(), 10), 2, 6)))

# islice consumes exactly `stop` items from the source, not one extra.
side = []
def counter():
    n = 0
    while True:
        side.append(n)
        yield n
        n += 1
print(list(itertools.islice(counter(), 4)), side)

# next() on a lazy islice, interleaved.
gen = itertools.islice(itertools.count(100), 50)
print(next(gen), next(gen), next(gen))

# yield-from over a lazy pipeline.
def deleg():
    yield from take(itertools.count(), 3)
    yield 99
print(list(deleg()))
