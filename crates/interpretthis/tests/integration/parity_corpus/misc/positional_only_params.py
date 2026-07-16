# Positional-only parameters (before `/`) cannot be filled by keyword: naming
# one is a TypeError, unless a **kwargs absorbs it (then the param binds
# positionally or reports as missing).

def f1(a, b, /, c):
    return (a, b, c)


print(f1(1, 2, 3), f1(1, 2, c=3))
try:
    f1(a=1, b=2, c=3)
except TypeError as e:
    print("ERR:", e)
try:
    f1(1, b=2, c=3)
except TypeError as e:
    print("ERR:", e)


def f2(a, /, **kw):
    return (a, kw)


print(f2(1), f2(1, a=2), f2(1, b=2))
try:
    f2(a=1)
except TypeError as e:
    print("ERR:", e)


def f3(a, b, /):
    return (a, b)


try:
    f3(1, b=2)
except TypeError as e:
    print("ERR:", e)


def f4(a, b=5, /, c=6):
    return (a, b, c)


print(f4(1), f4(1, 2), f4(1, 2, 3), f4(1, c=9))


def f5(a, b, /, c, *, d):
    return (a, b, c, d)


print(f5(1, 2, 3, d=4), f5(1, 2, c=3, d=4))
try:
    f5(1, 2, 3, 4)
except TypeError as e:
    print("ERR:", e)
