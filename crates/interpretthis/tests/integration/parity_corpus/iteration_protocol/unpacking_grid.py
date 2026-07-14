a, b, c = 1, 2, 3
print(a, b, c)
a, *b, c = [1, 2, 3, 4, 5]
print(a, b, c)
*a, b = "hello"
print(a, b)
(a, b), c = (1, 2), 3
print(a, b, c)
first, *rest = range(5)
print(first, rest)
for i, (k, v) in enumerate({"a": 1, "b": 2}.items()):
    print(i, k, v)
[x, y] = [10, 20]
print(x, y)
a = b = [1, 2]
a.append(3)
print(a, b)
head, *tail = [1]
print(head, tail)
