# Reading __class__ aliases type(x): it returns the value's type object, works
# on every value and through every read path (attribute, getattr, hasattr,
# f-string, nested chains, comprehensions). Writing __class__ stays blocked (a
# host SecurityError, covered by the security.rs integration test, not here).
print((5).__class__, "x".__class__, [].__class__, {}.__class__)
print((3.14).__class__, (True).__class__, (1 + 2j).__class__)
print(().__class__, b"x".__class__, frozenset().__class__)
print((5).__class__.__name__, "x".__class__.__name__, [].__class__.__name__)


class Foo:
    pass


f = Foo()
print(f.__class__.__name__, f.__class__ is Foo)
print(getattr(5, "__class__").__name__)
print(hasattr(5, "__class__"), hasattr(f, "__class__"))
print(f"{(5).__class__.__name__}")
print(isinstance(f, f.__class__))
print([x.__class__.__name__ for x in [1, "a", [], {}, (), 3.14]])


from enum import Enum


class Color(Enum):
    RED = 1


print(Color.RED.__class__.__name__, Color.RED.__class__ is Color)


class Base:
    pass


class Sub(Base):
    pass


print(Sub().__class__.__name__, Sub().__class__.__bases__ if False else "n/a")
print(type(5).__class__.__name__)
