def dict_comp_closures():
    return {i: (lambda: i) for i in range(3)}
d = dict_comp_closures()
print([d[k]() for k in sorted(d)])
def cond_capture():
    fns = []
    for i in range(5):
        if i % 2 == 0:
            fns.append(lambda: i)
    return [f() for f in fns]
print(cond_capture())
def nested_func_capture():
    result = []
    for i in range(3):
        def inner():
            return i * 100
        result.append(inner)
    return [f() for f in result]
print(nested_func_capture())
def mixed():
    x = 1
    fns = [lambda: x]
    x = 2
    fns.append(lambda: x)
    x = 3
    return [f() for f in fns]
print(mixed())
def two_vars():
    a, b = 1, 2
    f = lambda: (a, b)
    a, b = 10, 20
    return f()
print(two_vars())
def counter_factory():
    funcs = []
    for i in range(3):
        funcs.append(lambda n=i: n)
    return funcs
print([f() for f in counter_factory()])
def while_loop():
    fns = []
    i = 0
    while i < 3:
        fns.append(lambda: i)
        i += 1
    return [f() for f in fns]
print(while_loop())
def tuple_target():
    fns = []
    for (a, b) in [(1, 2), (3, 4)]:
        fns.append(lambda: a + b)
    return [f() for f in fns]
print(tuple_target())
def reads_after():
    data = [1, 2, 3]
    f = lambda: sum(data)
    data.append(4)
    return f()
print(reads_after())
def rebind_list():
    data = [1, 2, 3]
    f = lambda: data
    data = [4, 5, 6]
    return f()
print(rebind_list())
def enclosing_used_in_body():
    total = 0
    adders = []
    for i in range(3):
        adders.append(lambda x: x + i + total)
    total = 100
    return [a(0) for a in adders]
print(enclosing_used_in_body())
