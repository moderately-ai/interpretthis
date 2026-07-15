a, b, c = 1, 2, 3
print(a, b, c)
a, b = b, a
print(a, b)
x = y = z = 0
print(x, y, z)
first, *rest = [1, 2, 3, 4, 5]
print(first, rest)
*init, last = [1, 2, 3, 4]
print(init, last)
a, (b, c), d = 1, (2, 3), 4
print(a, b, c, d)
(a, b), c = [1, 2], 3
print(a, b, c)
for i, (j, k) in enumerate([(1, 2), (3, 4)]):
    print(i, j, k)
matrix = [[1, 2], [3, 4]]
(a, b), (c, d) = matrix
print(a, b, c, d)
head, *tail = "hello"
print(head, tail)
