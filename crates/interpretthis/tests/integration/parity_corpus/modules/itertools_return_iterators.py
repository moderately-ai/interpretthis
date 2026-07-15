# itertools producers return one-shot iterators, not lists: next() advances
# them, a second pass sees only the remainder, and they are not subscriptable.
import itertools as it

c = it.chain([1, 2], [3, 4])
print(next(c))
print(list(c))

print(list(it.islice(range(10), 2, 8, 2)))
print(list(it.accumulate([1, 2, 3, 4])))
print(list(it.accumulate([1, 2, 3, 4], initial=100)))
print(list(it.combinations([1, 2, 3], 2)))
print(list(it.permutations([1, 2], 2)))
print(list(it.product([1, 2], [3, 4])))
print(list(it.starmap(pow, [(2, 3), (3, 2)])))
print(list(it.takewhile(lambda x: x < 3, [1, 2, 3, 1])))
print(list(it.dropwhile(lambda x: x < 3, [1, 2, 3, 1])))
print(list(it.compress("abcd", [1, 0, 1, 0])))
print(list(it.filterfalse(lambda x: x % 2, range(6))))
print([list(g) for k, g in it.groupby("aabbcc")])

r = it.repeat("x", 3)
print(next(r))
print(list(r))

a, b = it.tee([1, 2, 3])
print(list(a), list(b))

for obj in (it.chain([1], [2]), it.islice(range(3), 2), it.repeat("z", 2)):
    try:
        obj[0]
    except TypeError as e:
        print(type(e).__name__)
