def f(a, b, c):
    return a + b + c
args = [1, 2, 3]
print(f(*args))
kw = {"a": 1, "b": 2, "c": 3}
print(f(**kw))
def g(*args, **kwargs):
    return sum(args) + sum(kwargs.values())
print(g(1, 2, x=3, y=4))
def h(first, *rest):
    return (first, rest)
print(h(1, 2, 3, 4))
a, *b, c = [1, 2, 3, 4, 5]
print(a, b, c)
print([*range(3), *range(3)])
