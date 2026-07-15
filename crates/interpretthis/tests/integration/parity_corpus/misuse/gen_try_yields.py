# Yields anywhere in a single try — body, except handler, else, finally —
# each resume at their own statement, and send() flows through.
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
def yield_in_finally():
    try:
        yield "body"
    finally:
        yield "cleanup"
print(list(yield_in_finally()))
def else_yield():
    try:
        yield "try"
    except ValueError:
        pass
    else:
        yield "else"
print(list(else_yield()))
def send_in_try():
    try:
        x = yield 1
        y = yield x + 1
        yield y + 1
    finally:
        print("done")
g = send_in_try()
print(next(g))
print(g.send(10))
g.close()
