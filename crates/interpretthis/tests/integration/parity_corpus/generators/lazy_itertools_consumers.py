# takewhile/dropwhile/filterfalse/starmap/accumulate/pairwise/compress over a
# LAZY input step it one item at a time (a generator that consumes on demand),
# so an infinite source does not hang and loop-var closures capture their
# interleaved value. Finite and non-lazy inputs keep their exact values.
import itertools
c = itertools.count

def sq_pairs():
    x = 1
    while True:
        yield (x, 2)
        x += 1

# Infinite sources, bounded by islice.
print(list(itertools.islice(itertools.dropwhile(lambda x: x < 3, c()), 4)))
print(list(itertools.islice(itertools.compress(c(), itertools.cycle([1, 0])), 3)))
print(list(itertools.islice(itertools.filterfalse(lambda x: x % 2, c()), 4)))
print(list(itertools.islice(itertools.accumulate(c()), 5)))
print(list(itertools.islice(itertools.accumulate(c(1), lambda a, b: a * b), 4)))
print(list(itertools.islice(itertools.starmap(pow, sq_pairs()), 3)))
print(list(itertools.islice(itertools.pairwise(c()), 3)))
print(list(itertools.takewhile(lambda x: x < 5, c())))

# Finite generators — exact values preserved.
g = lambda: (x for x in range(6))
print(list(itertools.dropwhile(lambda x: x < 3, g())))
print(list(itertools.compress(g(), [1, 0, 1, 0, 1, 1])))
print(list(itertools.filterfalse(lambda x: x % 2, g())))
print(list(itertools.accumulate(g())))
print(list(itertools.accumulate(g(), initial=100)))
print(list(itertools.starmap(lambda a, b: a * b, ((x, x) for x in range(1, 4)))))
print(list(itertools.pairwise(g())))
print(list(itertools.takewhile(lambda x: x < 4, g())))

# Loop-variable closures pulled interleaved through a lazy itertools consumer.
print([f() for f in itertools.takewhile(lambda f: True, (lambda: k for k in range(3)))])
print([f() for f in itertools.dropwhile(lambda f: False, (lambda: k for k in range(3)))])
print([f() for f in itertools.filterfalse(lambda f: False, (lambda: k for k in range(3)))])

# Non-lazy list inputs are unchanged.
print(list(itertools.dropwhile(lambda x: x < 2, [1, 2, 3, 4])))
print(list(itertools.compress([1, 2, 3, 4], [1, 0, 1, 1])))
print(list(itertools.filterfalse(lambda x: x % 2, [1, 2, 3, 4, 5])))
print(list(itertools.pairwise([1, 2, 3])))
print(list(itertools.accumulate([1, 2, 3, 4])))
print(list(itertools.starmap(lambda a, b: a + b, [(1, 2), (3, 4)])))
