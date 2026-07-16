a, *b, c = [1, 2, 3, 4, 5]
print(a, b, c)
*x, y = [1, 2, 3]; print(x, y)
p, *q = "hello"; print(p, q)
first, (second, third) = [1, [2, 3]]; print(first, second, third)
d = {"a": 1, "b": 2}
print([*d], {*"abc"}, (*[1, 2], *[3, 4]))
print({**d, "c": 3})
def f(*args, **kwargs): return args, kwargs
print(f(1, 2, x=3))
print(f(*[1, 2], **{"y": 4}))
[m, n] = (5, 6); print(m, n)
head, *tail = range(5); print(head, tail)
a = b = c = 10; print(a, b, c)
(w := 5); print(w)
print([y := 2, y + 1])
