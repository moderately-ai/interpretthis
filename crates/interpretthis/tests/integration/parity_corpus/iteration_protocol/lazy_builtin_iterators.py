# zip/map/filter/enumerate/reversed return one-shot iterators, not lists:
# next() advances them, a second pass sees only the remainder, and they are
# not subscriptable.
z = zip([1, 2, 3], [4, 5, 6])
print(next(z))
print(next(z))
print(list(z))

m = map(str, [1, 2, 3])
print(next(m))
print(list(m))

print(isinstance(zip([1], [2]), list))
print(isinstance(map(str, [1]), list))
print(isinstance(filter(None, [1]), list))
print(isinstance(enumerate([1]), list))
print(isinstance(reversed([1]), list))

for obj in (zip([1], [2]), map(str, [1]), filter(None, [1]), enumerate([1]), reversed([1])):
    try:
        obj[0]
    except TypeError as e:
        print(type(e).__name__)

# The eager consumers still see every element.
print(list(reversed([1, 2, 3])))
print(list(enumerate("ab", start=1)))
print(list(map(lambda x: x * 2, [1, 2, 3])))
print(list(filter(lambda x: x % 2, range(6))))
print(dict(zip("abc", [1, 2, 3])))
print(sorted(zip([2, 1], [3, 4])))
print(sum(map(int, ["1", "2", "3"])))
a, b = zip(*[(1, 2), (3, 4)])
print(a, b)
