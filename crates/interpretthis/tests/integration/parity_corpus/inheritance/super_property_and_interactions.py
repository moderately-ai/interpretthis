# generator delegating + send/throw propagation
def sub():
    while True:
        x = yield
        if x is None: return "done"
        print("sub got", x)
def deleg():
    r = yield from sub()
    print("deleg:", r)
    yield "after"
g = deleg()
next(g)
g.send(1)
g.send(2)
print(g.send(None))
# exception in with inside generator
def gen_with():
    class CM:
        def __enter__(self): return self
        def __exit__(self, *a): print("exit", a[0].__name__ if a[0] else None); return False
    with CM():
        yield 1
        yield 2
gw = gen_with()
print(next(gw))
gw.close()
# property returning a generator
class Container:
    def __init__(self, items): self._items = items
    @property
    def doubled(self): return (x * 2 for x in self._items)
c = Container([1, 2, 3])
print(list(c.doubled))
# dataclass field default_factory closing over data
from dataclasses import dataclass, field
BASE = [1, 2]
@dataclass
class D:
    vals: list = field(default_factory=lambda: BASE.copy())
print(D().vals, D().vals)
# diamond inheritance with property + super
class A:
    @property
    def name(self): return "A"
class B(A):
    @property
    def name(self): return "B+" + super().name
class C2(A):
    @property
    def name(self): return "C+" + super().name
class Dia(B, C2):
    @property
    def name(self): return "D+" + super().name
print(Dia().name)
print([c.__name__ for c in Dia.mro()])
# comprehension with walrus + nested
print([y for x in range(5) if (y := x * x) > 4])
# generator in comprehension in method with closure
class Calc:
    def __init__(self, n): self.n = n
    def compute(self):
        return [sum(i for i in range(k)) for k in range(self.n)]
print(Calc(4).compute())
