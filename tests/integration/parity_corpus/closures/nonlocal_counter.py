# Pins: nonlocal closure mutation — `inc` mutates `n` in the
# enclosing `counter` scope, and successive calls observe the
# updated value. Customer pattern for accumulators / hit counters.
def counter():
    n = 0
    def inc():
        nonlocal n
        n += 1
        return n
    return inc

c = counter()
print(c(), c(), c())
