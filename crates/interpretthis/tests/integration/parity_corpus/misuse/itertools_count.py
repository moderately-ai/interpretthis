import itertools
print(list(itertools.islice(itertools.count(10), 3)))
print(list(itertools.islice(itertools.count(0, 2), 4)))
print(list(itertools.islice(itertools.cycle([1, 2]), 5)))
print(list(itertools.takewhile(lambda x: x < 5, itertools.count(1))))
print(list(itertools.islice(itertools.count(2.5, 0.5), 3)))
print(list(itertools.islice(itertools.repeat("x"), 3)))
print(list(itertools.islice(itertools.cycle([]), 3)))
c = itertools.count()
print(next(c), next(c), next(c))
out = []
for i in itertools.count(1):
    if i > 4:
        break
    out.append(i)
print(out)
print(list(zip(itertools.count(), "abc")))
print(dict(zip("xyz", itertools.count(1))))
