import itertools
# for-in-for over infinite outer
def g():
    for i in itertools.count():
        for j in range(3):
            yield (i, j)
print(list(itertools.islice(g(), 8)))
# for-in-for finite (was already eager-but-correct; verify still works)
def h():
    for i in range(3):
        for j in range(2):
            yield i * 10 + j
print(list(h()))
# for-in-for with state and next
def k():
    for i in range(100):
        for j in range(2):
            yield (i, j)
gen = k()
print([next(gen) for _ in range(5)])
# for-in-for with if
def m():
    for i in range(4):
        for j in range(4):
            if i == j:
                yield i
print(list(m()))
# cross-kind still works
def n():
    while True:
        for j in range(2):
            yield j
print(list(itertools.islice(n(), 5)))
