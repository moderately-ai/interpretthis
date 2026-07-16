import itertools


def tn(x):
    return type(x).__name__


# Functional builtin iterators report their own CPython type name, not the
# generic "generator".
print(tn(map(str, [1])))
print(tn(filter(None, [1])))
print(tn(zip([1], [2])))
print(tn(zip()))
print(tn(enumerate([1])))
# Generator expressions stay "generator".
print(tn(x for x in [1]))
# itertools producers each carry their own type name.
print(tn(itertools.count()))
print(tn(itertools.cycle([1])))
print(tn(itertools.repeat(1)))
print(tn(itertools.chain([1])))
print(tn(itertools.chain.from_iterable([[1]])))
print(tn(itertools.islice([1], 1)))
print(tn(itertools.groupby("a")))
print(tn(itertools.accumulate([1])))
print(tn(itertools.combinations([1, 2], 1)))
print(tn(itertools.combinations_with_replacement([1, 2], 1)))
print(tn(itertools.permutations([1, 2])))
print(tn(itertools.product([1])))
print(tn(itertools.starmap(print, [])))
print(tn(itertools.takewhile(bool, [])))
print(tn(itertools.dropwhile(bool, [])))
print(tn(itertools.filterfalse(bool, [])))
print(tn(itertools.compress([], [])))
print(tn(itertools.pairwise([1])))
print(tn(itertools.zip_longest([1])))
print(tn(itertools.batched([1], 1)))
print(tn(itertools.tee([1])[0]))
