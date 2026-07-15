log = []
def gen():
    for i in range(3):
        log.append(f"before {i}")
        yield i
        log.append(f"after {i}")
g = gen()
print(next(g))
print(log[:])
print(next(g))
print(log[:])
def counter():
    n = 0
    while n < 3:
        log.append(f"yield {n}")
        yield n
        n += 1
log.clear()
c = counter()
next(c)
print(log[:])
next(c)
print(log[:])
