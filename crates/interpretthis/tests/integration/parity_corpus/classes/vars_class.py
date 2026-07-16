# vars(Cls) returns the class's own namespace (attributes + methods +
# static/class methods + properties). Safe in the sandbox: these are all
# in-sandbox values already reachable via Cls.name, and none expose a path to
# the host (the escape primitives __subclasses__/__globals__/__code__ stay
# blocked). Residual: CPython's vars(Cls) is a read-only mappingproxy while ours
# is a plain dict, so `type(vars(Cls)).__name__` differs ("dict" vs
# "mappingproxy") and mutating the result does not raise — every read operation
# below matches. Key order follows the sorted namespace, so only order-
# independent queries are asserted.
class C:
    x = 1
    y = 2

    def m(self):
        pass

    @staticmethod
    def s():
        pass

    @classmethod
    def c(cls):
        pass

    @property
    def p(self):
        return 1


print(sorted(vars(C).keys() & {"x", "y", "m", "s", "c", "p"}))
print(vars(C).get("x"), vars(C).get("y"))
print("m" in vars(C), "s" in vars(C), "p" in vars(C), "missing" in vars(C))
print(callable(vars(C)["m"]))

# Dynamically-created classes expose their namespace dict too.
d = type("D", (), {"a": 1, "b": 2})
print(vars(d).get("a"), vars(d).get("b"))
print(sorted(vars(d).keys() & {"a", "b"}))

# vars(instance) still returns the instance's own fields.
inst = C()
inst.attr = 10
print(vars(inst))

# A non-namespace object still raises exactly as CPython.
for bad in (5, [1, 2], "x"):
    try:
        vars(bad)
    except TypeError as e:
        print("TypeError:", e)
