import itertools
log = []
def g():
    n = 0
    while True:
        if n % 3 == 0:
            log.append(n)
            yield n
        n += 1
print(list(itertools.islice(g(), 5)))
print(log)
# Multi-statement branch, top-level.
trace = []
def multi():
    if True:
        trace.append("a")
        yield 1
        trace.append("b")
        yield 2
        trace.append("c")
g = multi()
print(next(g), trace[:])
print(next(g), trace[:])
print(list(g), trace[:])
# if inside for, with side effects.
seen = []
def filt():
    for i in range(6):
        if i % 2:
            seen.append(i)
            yield i * 100
print(list(filt()), seen)
# if inside try, with finally.
order = []
def guarded():
    try:
        if True:
            order.append("body")
            yield "x"
            order.append("after")
    finally:
        order.append("finally")
print(list(guarded()), order)
