# CPython private-name mangling: `__x` inside a class body is rewritten to
# `_ClassName__x` at compile time. The load-bearing consequence is that a
# subclass's `__x` does not clobber a parent's.
class A:
    def __init__(self): self.__x = "A"
    def get_a(self): return self.__x
class B(A):
    def __init__(self):
        super().__init__()
        self.__x = "B"
    def get_b(self): return self.__x
b = B()
print(b.get_a(), b.get_b())

# Mangled method names, class-private attrs, and hasattr with the mangled name.
class M:
    __attr = "class-private"
    def __init__(self): self.__inst = 1
    def __method(self): return "mangled method"
    def call(self): return self.__method()
    def get_class_attr(self): return self.__attr
m = M()
print(m.call(), m.get_class_attr())
print(hasattr(m, "_M__inst"), hasattr(m, "__inst"))
print(hasattr(M, "_M__attr"))

# `obj.__x` where obj is not self still mangles to the enclosing class.
class Accessor:
    def peek(self, other): return other.__secret
class Holder:
    def __init__(self): self.__secret = "H"
h = Holder()
try:
    Accessor().peek(h)
except AttributeError:
    print("mangled to _Accessor__secret, not found on Holder")
print(hasattr(h, "_Holder__secret"))

# Mangling reaches into comprehensions, lambdas, nested functions, and defaults
# that textually occur in the class body.
class WithComp:
    __base = 10
    def compute(self): return [self.__base + i for i in range(3)]
print(WithComp().compute())

class WithLambda:
    __k = 5
    def make(self):
        f = lambda: self.__k
        return f()
print(WithLambda().make())

class Defaults:
    __D = 100
    def m(self, x=__D): return x
print(Defaults().m())

class NestedName:
    __x = "outer"
    def outer_m(self):
        def inner_fn(): return self.__x
        return inner_fn()
print(NestedName().outer_m())

# `_single` (one underscore) and `__dunder__` (trailing dunder) are NOT mangled.
class Edge:
    def __init__(self):
        self._single = 1
        self.__dunder__ = 2
        self.__mangled = 3
    def check(self): return self._single, self.__dunder__, self.__mangled
e = Edge()
print(e.check())
print(hasattr(e, "_single"), hasattr(e, "__dunder__"), hasattr(e, "_Edge__mangled"))
print(hasattr(e, "__mangled"))
