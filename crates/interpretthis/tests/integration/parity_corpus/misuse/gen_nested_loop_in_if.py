def g():
    for i in range(3):
        if i > 0:
            for j in range(i):
                yield (i, j)
print(list(g()))
def h():
    if True:
        for x in range(4):
            yield x * x
print(list(h()))
def k():
    for a in range(3):
        for b in range(2):
            yield a * 10 + b
print(list(k()))
def m():
    for x in range(5):
        if x % 2 == 0:
            yield x
print(list(m()))
def w():
    n = 0
    while n < 4:
        if n % 2 == 0:
            yield n
        n += 1
print(list(w()))
