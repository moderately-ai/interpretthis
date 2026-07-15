from functools import cached_property
class C:
    def __init__(self, v):
        self._v = v
        self.calls = 0
    @cached_property
    def doubled(self):
        self.calls += 1
        return self._v * 2
c = C(10)
print(c.doubled)
print(c.doubled)
print(c.calls)
c2 = C(5)
print(c2.doubled, c.doubled)
print(c2.calls, c.calls)
import functools
class D:
    @functools.cached_property
    def x(self):
        return 99
d = D()
print(d.x, d.x)
class Regular:
    def __init__(self): self._n = 3
    @property
    def n(self): return self._n * 10
r = Regular()
print(r.n, r.n)
