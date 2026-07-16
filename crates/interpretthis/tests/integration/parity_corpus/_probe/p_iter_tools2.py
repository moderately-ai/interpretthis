print(list(map(str, range(3))))
g = (x**2 for x in range(5))
print(next(g), next(g), list(g))
print(sum(1 for _ in "hello"))
def counter():
    i = 0
    while True:
        yield i; i += 1
c = counter()
print([next(c) for _ in range(5)])
print(list(zip(range(3), range(5))))
it = iter([1, 2, 3])
print(next(it), next(it))
try:
    next(iter([]))
except StopIteration:
    print("empty")
print(list(filter(None, [0, 1, 2, 0, 3])))
print(tuple(enumerate("xy")))
print(max(enumerate([3, 1, 4]), key=lambda p: p[1]))
print(sorted(zip([3, 1, 2], "cab")))
nested = [[1, 2], [3, 4], [5]]
print([x for sub in nested for x in sub])
print(dict(enumerate("abc")))
print(list(map(lambda x, y: x + y, [1, 2], [10, 20])))
