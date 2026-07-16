# `__name__`/`__qualname__` and the CPython-shaped names that appear in
# argument-count TypeErrors: functions carry the simple name, methods and
# nested defs carry the dotted qualname (`C.method`, `outer.<locals>.inner`).

def outer():
    def inner(a, b):
        return a + b
    return inner


print(outer.__name__, outer.__qualname__)
print(outer().__name__, outer().__qualname__)
try:
    outer()(1)
except TypeError as e:
    print(e)


class C:
    def method(self, a, b):
        pass

    @staticmethod
    def stat(a, b):
        pass

    @classmethod
    def cm(cls, a, b):
        pass


print(C.method.__name__, C.method.__qualname__)
try:
    C().method(1)
except TypeError as e:
    print(e)
try:
    C.stat(1)
except TypeError as e:
    print(e)
try:
    C.cm(1)
except TypeError as e:
    print(e)


class Outer:
    class Inner:
        def m(self, a, b):
            pass


print(Outer.Inner.m.__qualname__)
try:
    Outer.Inner().m(1)
except TypeError as e:
    print(e)


f = lambda a, b: a
print(f.__name__, f.__qualname__)
g = lambda: (lambda a, b: a)
print(g().__qualname__)
try:
    g()(1)
except TypeError as e:
    print(e)


def make():
    return lambda x, y: x


print(make().__qualname__)


# Error-message wording for %-formatting and unknown codecs.
def show(fn):
    try:
        fn()
    except Exception as e:
        print(type(e).__name__, "::", e)


show(lambda: "%d" % "str")
show(lambda: "%i" % "str")
show(lambda: "%x" % "str")
show(lambda: "%f" % "str")
show(lambda: "abc".encode("bogus"))
show(lambda: b"abc".decode("bogus"))
show(lambda: bytes("abc", "bogus"))
