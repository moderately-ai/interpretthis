import itertools
# for-containing-while (infinite outer via itertools consumers won't work since for over finite;
# but for over infinite gen containing while):
def a():
    while True:
        for j in range(3):
            yield j
print(list(itertools.islice(a(), 7)))
# while-containing-for with side effects and state
def b():
    n = 0
    while True:
        for j in range(2):
            yield (n, j)
        n += 1
print(list(itertools.islice(b(), 6)))
# for-in-range containing while (finite)
def c():
    for i in range(3):
        k = 0
        while k < i:
            yield (i, k)
            k += 1
print(list(c()))
# while containing for containing if
def d():
    n = 0
    while n < 3:
        for j in range(3):
            if j != n:
                yield (n, j)
        n += 1
print(list(d()))
# while-True with for and break-out via StopIteration-like (close)
def e():
    while True:
        for j in range(4):
            yield j
g = e()
print([next(g) for _ in range(9)])
# nested for-in-for stays eager but finite works
def f():
    for i in range(3):
        for j in range(i):
            yield i*10+j
print(list(f()))
# while-for with send
def acc():
    total = 0
    while True:
        for _ in range(2):
            x = yield total
            total += x
a2 = acc()
print(next(a2))
print(a2.send(10))
print(a2.send(5))
