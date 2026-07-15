def sub():
    x = yield "a"
    print("sub got", x)
    y = yield "b"
    print("sub got", y)
    return "sub-return"
def deleg():
    result = yield from sub()
    print("deleg result", result)
    yield "c"
g = deleg()
print(next(g))
print(g.send(1))
print(g.send(2))
print(next(g))

def sub_throw():
    try:
        yield 1
        yield 2
    except ValueError as e:
        print("sub caught", e)
        yield 99
def deleg_throw():
    yield from sub_throw()
    yield "after"
g2 = deleg_throw()
print(next(g2))
print(g2.throw(ValueError("boom")))
print(next(g2))

def inner():
    yield from range(3)
    yield from [10, 20]
print(list(inner()))

def counter_gen():
    total = 0
    while total < 3:
        total += 1
        yield total
    return total
def wrapper():
    final = yield from counter_gen()
    print("final", final)
print(list(wrapper()))
