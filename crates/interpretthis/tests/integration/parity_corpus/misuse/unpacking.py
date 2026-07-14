a = [1, 2]
b = [3, 4]
print([*a, *b, 5])
print({**{"x": 1}, **{"y": 2}})
def f(*args, **kw):
    return args, sorted(kw.items())
print(f(*a, z=1, w=2))
first, *rest = [1, 2, 3, 4]
print(first, rest)
*init, last = [1, 2, 3]
print(init, last)
x, (y, z) = 1, (2, 3)
print(x, y, z)
