# hasattr must agree with getattr for function / lambda / class / exception
# introspection — they resolve through one path, so no drift (e.g. a lambda's
# __doc__ / __call__ were previously visible to getattr but not hasattr).
def f(a, b=1, *, c=2):
    """doc"""
    return a


def check(obj, names):
    for n in names:
        h = hasattr(obj, n)
        try:
            getattr(obj, n)
            g = True
        except AttributeError:
            g = False
        if h != g:
            print(f"DRIFT {n}: hasattr={h} getattr={g}")
    return "ok"


print(check(f, ["__name__", "__qualname__", "__doc__", "__call__", "__annotations__", "__defaults__", "__kwdefaults__", "missing"]))
print(check(lambda x: x, ["__name__", "__qualname__", "__doc__", "__call__", "missing"]))


class C:
    x = 1

    def m(self):
        pass


print(check(C, ["__name__", "__qualname__", "x", "m", "missing"]))
print(check(ValueError("boom"), ["args", "missing"]))
print(hasattr(f, "__defaults__"), hasattr(f, "__kwdefaults__"), hasattr(f, "__annotations__"))
print(hasattr(lambda: 1, "__doc__"), hasattr(lambda: 1, "__call__"))
