g = (x for x in range(5))
print(hasattr(g, "send"), hasattr(g, "throw"), hasattr(g, "close"))
print(hasattr(g, "__next__"), hasattr(g, "__iter__"))
print(callable(g.send), callable(g.close))
def mygen():
    x = yield 1
    yield x
m = mygen()
print(next(m))
print(m.send(99))
gen = (x*2 for x in range(3))
gen.close()
print("closed ok")
import itertools
c = itertools.count()
print(hasattr(c, "__next__"))
r = range(5)
print(hasattr(r, "send"), hasattr([], "send"))
