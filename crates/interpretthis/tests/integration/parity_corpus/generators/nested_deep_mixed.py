import itertools
# if-in-if then loops
def g():
    for i in itertools.count():
        if i % 2 == 0:
            if i % 4 == 0:
                for j in range(2):
                    yield (i, j)
print(list(itertools.islice(g(), 6)))
# top-level if containing infinite loop
def h():
    if True:
        for i in itertools.count():
            for j in range(2):
                yield (i, j)
print(list(itertools.islice(h(), 6)))
# loop -> for -> if -> while
def m():
    for i in itertools.count():
        for j in range(2):
            if j == 0:
                n = 0
                while n < 2:
                    yield (i, j, n)
                    n += 1
print(list(itertools.islice(m(), 8)))
# try inside loop stays eager (documented) - finite works
def t():
    for i in range(3):
        try:
            yield i
        finally:
            pass
print(list(t()))
