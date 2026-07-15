# Generator protocol: send, throw, close, return value in StopIteration.
def gen():
    x = yield 1
    y = yield x + 10
    return y * 2

g = gen()
print(next(g))
print(g.send(5))
try:
    g.send(100)
except StopIteration as e:
    print("stop:", e.value)

# throw into a generator
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

# close
def closer():
    try:
        yield 1
    finally:
        print("cleanup")

cl = closer()
print(next(cl))
cl.close()

# yield from delegation with return value
def inner():
    yield 1
    yield 2
    return "inner-done"

def outer():
    result = yield from inner()
    yield f"got {result}"

print(list(outer()))

# generator expression laziness and reuse
squares = (x * x for x in range(5))
print(next(squares), next(squares), list(squares))
