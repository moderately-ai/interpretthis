def echo():
    while True:
        received = yield
        print("got", received)
g = echo()
next(g)
g.send("a")
g.send("b")
def accumulator():
    total = 0
    while True:
        x = yield total
        if x is None:
            break
        total += x
a = accumulator()
print(next(a))
print(a.send(10))
print(a.send(5))
def catcher():
    try:
        yield 1
    except ValueError as e:
        yield f"caught {e}"
    yield 2
c = catcher()
print(next(c))
print(c.throw(ValueError("boom")))
print(next(c))
