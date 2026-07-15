class R:
    def __init__(self, v): self.v = v
    def __radd__(self, o): return f"radd({o}+{self.v})"
    def __rsub__(self, o): return f"rsub({o}-{self.v})"
    def __rmul__(self, o): return f"rmul({o}*{self.v})"
    def __rtruediv__(self, o): return f"rdiv({o}/{self.v})"
    def __rmod__(self, o): return f"rmod({o}%{self.v})"
    def __rpow__(self, o): return f"rpow({o}**{self.v})"
r = R(5)
print(3 + r, 3 - r, 3 * r)
print(10 / r, 10 % r, 2 ** r)
class NotImpl:
    def __add__(self, o): return NotImplemented
    def __radd__(self, o): return "fallback radd"
try:
    print(NotImpl() + NotImpl())
except TypeError:
    print("same-type: no reflected fallback")
class Both:
    def __init__(self, v): self.v = v
    def __add__(self, o): return f"add({self.v})"
    def __radd__(self, o): return f"radd({self.v})"
print(Both(1) + Both(2))
class Sub(Both): pass
print(Both(1) + Sub(2))
