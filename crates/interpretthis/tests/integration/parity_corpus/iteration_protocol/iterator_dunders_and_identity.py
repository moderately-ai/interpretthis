# Generators, lazy iterators, and itertools iterators expose the iterator
# protocol as methods and bound methods, and are identical only to themselves.
import itertools as it

g = (x for x in range(5))
print(g.__next__(), g.__next__())
m = g.__next__
print(m())

c = it.count(10, 2)
print(c.__next__(), c.__next__())
cm = c.__next__
print(cm())

z = zip([1, 2, 3], [4, 5, 6])
print(z.__next__())

# A generator expression supports send() as a full generator.
def gen():
    x = yield 1
    yield x + 1
gg = gen()
print(gg.send(None))
print(gg.send(10))

# __iter__ returns the iterator itself.
print(g.__iter__() is g)
print(iter(g) is g)
print(c.__iter__() is c)

# Identity: an iterator is identical only to itself.
print(g is g)
g2 = (x for x in range(5))
print(g is g2)
print(c is c)

# close() is accepted on a generator.
g3 = (x for x in range(3))
g3.close()
print("closed")
