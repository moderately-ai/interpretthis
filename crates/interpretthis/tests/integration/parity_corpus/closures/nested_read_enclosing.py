# A nested function reads a variable from its enclosing function's scope, and
# the returned closure keeps that binding after the outer call returns.
def outer():
    x = 10
    def inner():
        return x
    return inner


print(outer()())


def make_adder(n):
    def add(y):
        return n + y
    return add


print(make_adder(100)(5))
add3 = make_adder(3)
print(add3(1), add3(10))


# Reads see the value at call time, and multiple closures capture independently.
def build():
    funcs = []
    for i in (1, 2, 3):
        def f(base=i):
            return base * 10
        funcs.append(f)
    return funcs


print([g() for g in build()])
