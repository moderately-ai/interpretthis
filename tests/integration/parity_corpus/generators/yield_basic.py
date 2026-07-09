# Pins: yield in a function produces an iterable; list(gen()) /
# `for x in gen()` / `next(gen())` all work; yield from chains
# iterables; generators are usable as iterables for comprehensions.
def evens(n):
    for i in range(n):
        if i % 2 == 0:
            yield i

print(list(evens(10)))

total = 0
for x in evens(6):
    total += x
print(total)

g = evens(5)
print(next(g))
print(next(g))
print(next(g))

def chain(*xs):
    for x in xs:
        yield from x

print(list(chain([1, 2], [3, 4], [5])))

def numbers():
    for i in range(5):
        yield i

print([x * 2 for x in numbers()])
