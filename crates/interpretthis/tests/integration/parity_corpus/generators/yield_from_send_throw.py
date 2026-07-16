def inner():
    x = yield 1
    y = yield x + 10
    return x + y
def outer():
    result = yield from inner()
    yield f"got {result}"
o = outer()
print(next(o))
print(o.send(5))
print(o.send(100))
# throw into delegated subgenerator
def catcher():
    try:
        yield 1
        yield 2
    except ValueError:
        yield "caught in sub"
def delegator():
    yield from catcher()
    yield "after"
d = delegator()
print(next(d))
print(d.throw(ValueError))
print(next(d))
# generator close runs finally
def with_finally():
    try:
        yield 1
        yield 2
    finally:
        print("cleanup")
wf = with_finally()
print(next(wf))
wf.close()
# StopIteration value
def returns_value():
    yield 1
    return 42
rv = returns_value()
next(rv)
try:
    next(rv)
except StopIteration as e:
    print("value:", e.value)
# yield from a list/range/str
def yf():
    yield from [1, 2]
    yield from range(3, 5)
    yield from "ab"
print(list(yf()))
# nested yield from
def level2(): yield from [1, 2, 3]
def level1(): yield from level2()
def level0(): yield from level1()
print(list(level0()))
