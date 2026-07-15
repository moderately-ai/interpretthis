def echo():
    while True:
        received = yield
        print(f"got {received}")
g = echo()
next(g)
g.send("hello")
g.send("world")
def accumulator():
    total = 0
    while True:
        val = yield total
        total += val
a = accumulator()
print(next(a))
print(a.send(10))
print(a.send(5))
def gen_with_throw():
    try:
        yield 1
        yield 2
    except ValueError:
        yield "caught"
gt = gen_with_throw()
print(next(gt))
print(gt.throw(ValueError))
def sub():
    yield 1
    yield 2
    return 3
def delegating():
    result = yield from sub()
    yield result
print(list(delegating()))
def finite():
    yield from [10, 20, 30]
print(list(finite()))
