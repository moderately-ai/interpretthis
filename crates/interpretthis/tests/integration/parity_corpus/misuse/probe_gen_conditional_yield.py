import itertools
# Filtered infinite stream: `while True: if cond: yield` now suspends lazily.
def evens():
    n = 0
    while True:
        if n % 2 == 0:
            yield n
        n += 1
print(list(itertools.islice(evens(), 5)))
# if/else both yielding, finite.
def signed():
    n = 0
    while n < 6:
        if n % 2 == 0:
            yield n
        else:
            yield -n
        n += 1
print(list(signed()))
# Conditional yield with send.
def echo_positive():
    while True:
        v = yield
        if v is not None and v > 0:
            yield v * 10
g = echo_positive()
next(g)
print(g.send(5))
# Nested if leading to yield.
def nested():
    n = 0
    while n < 10:
        if n > 2:
            if n < 6:
                yield n
        n += 1
print(list(nested()))
# Statements AFTER the yield run once (not re-executed).
seen = []
def after():
    n = 0
    while n < 4:
        if n % 2 == 0:
            yield n
            seen.append(n)
        n += 1
print(list(after()), seen)
