def f(a, b=2, *args, c, d=4, **kwargs):
    return (a, b, args, c, d, kwargs)
print(f(1, c=3))
print(f(1, 2, 3, 4, c=5, e=6))
print(f(1, c=3, x=10, y=20))
def g(*args, **kwargs):
    return (args, kwargs)
print(g(1, 2, 3, x=1, y=2))
print(g())
def h(a, b, c):
    return a + b + c
print(h(*[1, 2], 3), h(**{"a": 1, "b": 2, "c": 3}))
args = [1, 2, 3]
print(h(*args))
kw = {"b": 2, "c": 3}
print(h(1, **kw))
def defaults(x, y=[]):
    y.append(x)
    return y
print(defaults(1), defaults(2))
