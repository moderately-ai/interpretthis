# Type objects are instances of `type`; every callable exposes `__call__`
# (and `hasattr`/`callable`/`getattr` stay consistent about it).

print(isinstance(int, type), isinstance(str, type), isinstance(type, type))
print(isinstance(object, type), isinstance(ValueError, type), isinstance(Exception, type))
print(isinstance(len, type), isinstance(3, type), isinstance("x", type), isinstance([], type))


class C:
    pass


class D(C):
    pass


print(isinstance(C, type), isinstance(D, type), isinstance(C(), type))
print(issubclass(D, C), issubclass(C, D), issubclass(bool, int))


def f():
    pass


g = lambda: 1


class Callable:
    def __call__(self):
        return 1


print(callable(f), callable(g), callable(int), callable(len), callable(Callable))
print(callable(Callable()), callable(3), callable("x"), callable([]))
print(hasattr(f, "__call__"), hasattr(g, "__call__"), hasattr(3, "__call__"))
print(hasattr(int, "__call__"), hasattr(len, "__call__"), hasattr(Callable(), "__call__"))
print(callable(getattr(f, "__call__", None)), callable(getattr(g, "__call__", None)))
print(getattr(3, "__call__", "missing"))
