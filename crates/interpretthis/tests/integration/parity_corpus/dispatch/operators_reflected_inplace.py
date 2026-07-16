class V:
    def __init__(self, n): self.n = n
    def __add__(self, o): return V(self.n + (o.n if isinstance(o, V) else o))
    def __radd__(self, o): return V(o + self.n)
    def __mul__(self, o): return V(self.n * o)
    def __rmul__(self, o): return V(o * self.n)
    def __sub__(self, o): return V(self.n - o)
    def __rsub__(self, o): return V(o - self.n)
    def __iadd__(self, o): self.n += (o.n if isinstance(o, V) else o); return self
    def __repr__(self): return f"V({self.n})"
print(V(3) + 4, 4 + V(3))
print(V(3) * 2, 2 * V(3))
print(V(10) - 3, 20 - V(3))
v = V(5); v += 10; print(v)
v2 = V(1); orig = v2; v2 += V(9); print(orig is v2, v2)
class Num:
    def __init__(self, v): self.v = v
    def __add__(self, o): return NotImplemented
    def __radd__(self, o): return self.v + o if isinstance(o, int) else NotImplemented
print(5 + Num(10))
try:
    Num(1) + Num(2)
except TypeError as e:
    print("TE:", e)
class L:
    def __init__(self, d): self.d = list(d)
    def __imul__(self, n): self.d = self.d * n; return self
    def __repr__(self): return f"L({self.d})"
l = L([1, 2]); l *= 3; print(l)
class Idx:
    def __index__(self): return 3
print([0, 1, 2, 3, 4][Idx()], "abcdef"[Idx():])
print(list(range(10))[Idx()])
class Comp:
    def __init__(self, v): self.v = v
    def __lt__(self, o): return self.v < o.v
    def __eq__(self, o): return self.v == o.v
print(sorted([Comp(3), Comp(1), Comp(2)], key=lambda c: c.v) == [Comp(1), Comp(2), Comp(3)])
