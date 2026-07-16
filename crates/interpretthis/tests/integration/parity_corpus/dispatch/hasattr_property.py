# hasattr on a user instance is "getattr succeeds": it runs the real attribute
# resolution, so @property / @cached_property getters run (True unless the getter
# raises AttributeError), and methods / fields / __getattr__ all resolve through
# the one path — not a separate presence table.
class C:
    def __init__(self):
        self._v = 5

    @property
    def val(self):
        return self._v * 10

    def method(self):
        return 1


c = C()
print(c.val)
print(hasattr(c, "val"), hasattr(c, "method"), hasattr(c, "_v"))
print(hasattr(c, "missing"))


class D:
    @property
    def broken(self):
        raise AttributeError("nope")

    @property
    def ok(self):
        return 1


d = D()
print(hasattr(d, "ok"), hasattr(d, "broken"))


import functools


class E:
    @functools.cached_property
    def expensive(self):
        return 42


print(hasattr(E(), "expensive"))


class Dyn:
    def __getattr__(self, n):
        if n.startswith("dynamic_"):
            return n
        raise AttributeError(n)


dy = Dyn()
print(hasattr(dy, "dynamic_x"), hasattr(dy, "other"))


# getattr and hasattr agree for every name checked above.
for name in ("val", "method", "missing"):
    print(name, hasattr(c, name), getattr(c, name, "<none>") != "<none>")
