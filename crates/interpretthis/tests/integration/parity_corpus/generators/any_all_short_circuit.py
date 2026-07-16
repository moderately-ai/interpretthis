# any/all short-circuit — over a lazy iterator they step it one item at a time
# (so any(map(pred, count())) stops at the first truthy element instead of
# hanging while materialising the infinite source).
import itertools
c = itertools.count
print(any(map(lambda x: x > 5, c())))
print(all(map(lambda x: x < 5, c())))
print(any(filter(lambda x: x > 3, c())))
print(all(filter(lambda x: x < 3, c())))
print(any(enumerate(c())))
# direct lazy iterator (a user generator that is its own iterator)
def ones():
    while True:
        yield 1
print(any(ones()))
def falses():
    while True:
        yield 0
print(all(falses()))
# finite / short-circuit correctness
print(all([True, True, False]), any([0, 0, 1]))
print(all([]), any([]))
print(all(x < 10 for x in range(5)), any(x > 100 for x in range(5)))
print(any([]), all([1, 2, 3]))
