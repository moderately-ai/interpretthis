import itertools
# deep nesting through an if
def g():
    for i in itertools.count():
        if i % 2 == 0:
            for j in range(2):
                for k in range(2):
                    yield (i, j, k)
print(list(itertools.islice(g(), 8)))
# if between loops
def h():
    for i in itertools.count():
        for j in range(3):
            if j != 1:
                yield (i, j)
print(list(itertools.islice(h(), 6)))
