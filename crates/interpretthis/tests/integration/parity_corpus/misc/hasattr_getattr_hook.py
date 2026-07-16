# hasattr consults __getattr__ / __getattribute__ (it is defined as "getattr
# doesn't raise AttributeError"): a proxy that answers any name is has-everything,
# one that raises AttributeError for some names is selective, and a non-Attribute
# exception propagates out of hasattr.

class Dynamic:
    def __getattr__(self, name):
        return f"dynamic_{name}"


d = Dynamic()
print(hasattr(d, "anything"), hasattr(d, "foo"), d.foo)


class Selective:
    def __getattr__(self, name):
        if name.startswith("ok_"):
            return 1
        raise AttributeError(name)


s = Selective()
print(hasattr(s, "ok_x"), hasattr(s, "bad"))


class Raiser:
    def __getattr__(self, name):
        raise ValueError("boom")


try:
    hasattr(Raiser(), "x")
except ValueError:
    print("propagated")


class Normal:
    def __init__(self):
        self.a = 1


n = Normal()
print(hasattr(n, "a"), hasattr(n, "b"))
print(getattr(d, "computed"), getattr(s, "missing", "default"))
