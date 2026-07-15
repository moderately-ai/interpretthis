def counter():
    count = 0
    def inc():
        nonlocal count
        count += 1
        return count
    return inc
c = counter()
print(c(), c(), c())
def make_adders():
    return [lambda x, n=i: x + n for i in range(3)]
print([f(10) for f in make_adders()])
funcs = []
for i in range(3):
    def f(i=i):
        return i
    funcs.append(f)
print([f() for f in funcs])
def outer():
    x = "outer"
    def inner():
        return x
    return inner()
print(outer())
g = (lambda: (lambda y: y + 1))()
print(g(5))
