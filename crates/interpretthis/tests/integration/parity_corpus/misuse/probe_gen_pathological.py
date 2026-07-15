
shared = [10]
def watcher():
    while True:
        yield shared[0]
g = watcher()
print(next(g))
shared[0] = 20
print(next(g))
shared[0] = 30
print(next(g))
def stateful():
    total = 0
    while True:
        x = yield total
        if x is None:
            x = 1
        total += x
s = stateful()
print(next(s))
print(s.send(5))
print(s.send(10))
print(next(s))
def interleaved():
    i = 0
    while i < 5:
        received = yield i
        i += (received or 1)
it = interleaved()
print(next(it))
print(it.send(2))
print(next(it))
def early_return():
    n = 0
    while True:
        if n >= 3:
            return
        yield n
        n += 1
print(list(early_return()))
