import itertools
# for-for-for infinite outer
def g():
    for i in itertools.count():
        for j in range(2):
            for k in range(2):
                yield i*100+j*10+k
print(list(itertools.islice(g(), 10)))
# while-while-while infinite
def h():
    while True:
        b = 0
        while b < 2:
            c = 0
            while c < 2:
                yield (b, c)
                c += 1
            b += 1
print(list(itertools.islice(h(), 9)))
# mixed depth 3: for-while-for infinite
def m():
    for i in itertools.count():
        n = 0
        while n < 2:
            for k in range(2):
                yield (i, n, k)
            n += 1
print(list(itertools.islice(m(), 10)))
# depth 4
def q():
    for a in itertools.count():
        for b in range(2):
            for c in range(2):
                for d in range(2):
                    yield (a,b,c,d)
print(list(itertools.islice(q(), 10)))
