def running_sum():
    total = 0
    while True:
        val = yield total
        if val is not None:
            total += val
g = running_sum()
print(next(g))
print(g.send(10))
print(g.send(20))
print(g.send(5))
def coroutine():
    while True:
        x = yield
        print(f"received: {x}")
c = coroutine()
next(c)
c.send("hello")
c.send("world")
def limited():
    for i in range(3):
        received = yield i
        print(f"got {received}")
gen = limited()
print(next(gen))
print(gen.send("a"))
print(gen.send("b"))
