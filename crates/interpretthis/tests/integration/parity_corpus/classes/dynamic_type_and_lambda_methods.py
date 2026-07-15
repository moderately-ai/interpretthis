# 3-arg type() with methods in the namespace.
C = type("Dynamic", (), {})
print(C.__name__, type(C()).__name__)
obj = C()
setattr(obj, "x", 10)
print(obj.x)

D = type("WithAttrs", (), {"value": 42, "greet": lambda self: "hi", "double": lambda self, n: n * 2})
print(D.value)
d = D()
print(d.value, d.greet(), d.double(5))


class Base:
    def method(self):
        return "base"


E = type("Derived", (Base,), {"extra": lambda self: "extra"})
e = E()
print(e.method(), e.extra(), isinstance(e, Base))


# A lambda assigned in a regular class body is also a method.
class Regular:
    x = 1
    compute = lambda self, n: n + self.x
    label = lambda self: "regular"


r = Regular()
print(r.compute(10), r.label(), r.x)


# A plain function value in the namespace binds self too.
def standalone(self):
    return f"standalone {self.value}"


F = type("WithFunc", (), {"value": 99, "run": standalone})
print(F().run())
