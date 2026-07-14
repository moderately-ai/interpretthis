# A class body's chained and tuple/list-unpacking assignments all become class
# attributes. Regression: only a single `name = value` was recorded; `X, Y = 1, 2`
# and `a = b = 3` were silently dropped.
class C:
    X, Y = 1, 2
    a = b = 3
    (p, q), r = (4, 5), 6
    [m, n] = [7, 8]
    single = 9


print(C.X, C.Y)
print(C.a, C.b)
print(C.p, C.q, C.r)
print(C.m, C.n)
print(C.single)


# Arity mismatch in a class-body unpack raises ValueError (as at runtime).
try:
    class Bad:
        x, y = 1, 2, 3
except ValueError:
    print("ValueError")
