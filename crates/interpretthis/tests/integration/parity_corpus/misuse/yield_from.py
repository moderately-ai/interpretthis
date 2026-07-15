def inner():
    yield 1
    yield 2
def outer():
    yield 0
    yield from inner()
    yield 3
print(list(outer()))
def chained():
    yield from range(3)
    yield from [10, 20]
print(list(chained()))
def delegating():
    x = yield from sub()
    print("got", x)
def sub():
    yield 1
    return 99
g = delegating()
print(next(g))
try:
    next(g)
except StopIteration:
    pass
def sub2():
    yield "a"
    yield "b"
    yield "c"
    return "DONE"
def deleg2():
    result = yield from sub2()
    print("result:", result)
    yield "after"
g2 = deleg2()
print(list(g2))
