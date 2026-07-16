def make():
    x = 10
    f = lambda: x
    x = 20
    return f
print(make()())
def make2():
    fns = []
    for i in range(3):
        fns.append(lambda: i)
    return fns
print([f() for f in make2()])
def make3():
    x = 1
    def g():
        return x
    x = 99
    return g
print(make3()())
def make_adders_bug():
    return [lambda x: x + i for i in range(3)]
print([a(10) for a in make_adders_bug()])
def make_adders_ok():
    return [lambda x, n=i: x + n for i in range(3)]
print([a(10) for a in make_adders_ok()])
def counter():
    count = 0
    def inc():
        nonlocal count
        count += 1
        return count
    return inc
c = counter()
print(c(), c(), c())
def multi():
    funcs = []
    for i in range(3):
        def make(i=i):
            return i * 10
        funcs.append(make)
    return funcs
print([f() for f in multi()])
def escape():
    result = []
    for x in "abc":
        result.append(lambda: x)
    return [f() for f in result]
print(escape())
def nested_late():
    vals = []
    for i in range(3):
        for j in range(2):
            vals.append(lambda: (i, j))
    return [f() for f in vals]
print(nested_late())
def direct_comp():
    return [lambda: i for i in range(4)]
print([g() for g in direct_comp()])
