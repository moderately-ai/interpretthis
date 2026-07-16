def te(f):
    try:
        return f()
    except TypeError as e:
        return "TypeError: " + str(e)

def kwonly(a, *, b, c=3):
    return (a, b, c)
print(kwonly(1, b=2))
print(kwonly(1, b=2, c=4))
print(te(lambda: kwonly(1, 2)))
print(te(lambda: kwonly(1)))

def posonly(a, b, /, c):
    return (a, b, c)
print(posonly(1, 2, 3))
print(posonly(1, 2, c=3))
print(te(lambda: posonly(a=1, b=2, c=3)))

def variadic(*args, **kwargs):
    return (args, sorted(kwargs.items()))
print(variadic(1, 2, 3, x=4, y=5))
print(variadic())

def mixed(a, b=2, *args, c, d=4, **kwargs):
    return (a, b, args, c, d, sorted(kwargs.items()))
print(mixed(1, c=3))
print(mixed(1, 2, 3, 4, c=5, d=6, e=7))

def unknown_kw(a):
    return a
print(te(lambda: unknown_kw(1, x=2)))
print(te(lambda: unknown_kw(a=1, a=2) if False else unknown_kw(b=5)))

def dup(a):
    return a
print(te(lambda: dup(1, a=2)))

args_list = [1, 2, 3]
def takes3(a, b, c):
    return a + b + c
print(takes3(*args_list))
kw = {"a": 1, "b": 2, "c": 3}
print(takes3(**kw))
print(takes3(*[1], **{"b": 2, "c": 3}))

def default_mutable(x, lst=[]):
    lst.append(x)
    return lst
print(default_mutable(1), default_mutable(2))

def keyword_defaults(a, b="hello", c=None):
    return (a, b, c)
print(keyword_defaults(1))
print(keyword_defaults(1, c=5))

print((lambda *a: sum(a))(1, 2, 3, 4))
print((lambda **k: sorted(k.items()))(x=1, y=2))
print((lambda a, b=10: a * b)(5))

def call_with_star(f, *args, **kwargs):
    return f(*args, **kwargs)
print(call_with_star(takes3, 1, 2, 3))
print(call_with_star(pow, 2, 10))
