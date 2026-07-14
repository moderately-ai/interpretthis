# generator.throw injects an exception at the suspended yield so the generator's
# own try/except can handle it and resume.
def gen():
    try:
        yield 1
    except ValueError:
        yield 99
    yield 2


g = gen()
print(next(g))
print(g.throw(ValueError))   # caught -> yields 99
print(next(g))               # continues -> 2


# An uncaught throw propagates to the caller.
def gen2():
    yield 1
    yield 2


g2 = gen2()
print(next(g2))
try:
    g2.throw(RuntimeError("boom"))
except RuntimeError as e:
    print("propagated:", str(e))


# throw into a finished generator propagates.
def gen3():
    yield 1


g3 = gen3()
print(next(g3))
try:
    next(g3)
except StopIteration:
    pass
try:
    g3.throw(ValueError("x"))
except ValueError:
    print("finished-throw")
