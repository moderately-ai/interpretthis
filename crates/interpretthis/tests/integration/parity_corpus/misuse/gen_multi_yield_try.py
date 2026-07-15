def g():
    try:
        yield 1
        yield 2
        yield 3
    finally:
        print("cleanup")
it = g()
print(next(it))
print(next(it))
print("draining")
print(list(it))
def counter():
    n = 0
    try:
        while n < 3:
            n += 1
            yield n
    finally:
        print("counter done")
print(list(counter()))
def multi():
    try:
        yield "a"
        yield "b"
    except ValueError:
        yield "caught"
    finally:
        print("fin")
m = multi()
print(next(m))
print(m.throw(ValueError))
print(list(m))
