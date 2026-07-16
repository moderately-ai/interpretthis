import itertools
# while-in-while infinite outer
def g():
    while True:
        b = 0
        while b < 3:
            yield b
            b += 1
print(list(itertools.islice(g(), 8)))
# while-in-while with state
def h():
    a = 0
    while True:
        b = 0
        while b < 2:
            yield (a, b)
            b += 1
        a += 1
print(list(itertools.islice(h(), 6)))
# while-in-while finite
def k():
    a = 0
    while a < 3:
        b = 0
        while b < 2:
            yield a * 10 + b
            b += 1
        a += 1
print(list(k()))
# while containing for containing while would be depth 3 -> eager; skip
# mixed: while-in-for
def m():
    for i in range(3):
        n = 0
        while n < i:
            yield (i, n)
            n += 1
print(list(m()))
# while with send inside nested
def acc():
    total = 0
    while True:
        c = 0
        while c < 2:
            x = yield total
            total += x
            c += 1
a = acc()
print(next(a), a.send(10), a.send(5))
