class V:
    def __init__(self, v): self.v = v
    def __add__(self, o): return V(self.v + o.v)
    def __radd__(self, o): return V(self.v + o)
    def __mul__(self, o): return V(self.v * o)
    def __repr__(self): return f"V({self.v})"
    def __eq__(self, o): return isinstance(o, V) and self.v == o.v
    def __hash__(self): return hash(self.v)
    def __lt__(self, o): return self.v < o.v
    def __len__(self): return self.v
    def __getitem__(self, i): return self.v + i
    def __call__(self, x): return self.v * x
    def __neg__(self): return V(-self.v)
    def __abs__(self): return V(abs(self.v))
    def __contains__(self, x): return x == self.v
print(V(3) + V(4))
print(10 + V(5))
print(V(3) * 2)
print(V(3) == V(3), V(3) == V(4))
print(sorted([V(3), V(1), V(2)]))
print(len(V(5)))
print(V(10)[3])
print(V(4)(5))
print(-V(3), abs(V(-7)))
print(3 in V(3), 5 in V(3))
print({V(1): "a", V(2): "b"}[V(1)])
class Ctx:
    def __enter__(self): print("enter"); return self
    def __exit__(self, *a): print("exit", a[0]); return False
with Ctx() as c: print("body")
