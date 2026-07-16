# A single custom (non-builtin-shape) method decorator is evaluated and applied
# to the method as a normal callable, then stored as a class attribute so the
# descriptor protocol (__get__ / __set_name__) or plain function-as-method
# binding drives access. Covers descriptor-returning and function-returning
# decorators, decorator factories (@deco(arg)), and functools.wraps.
class LazyProp:
    def __init__(self, func):
        self.func = func

    def __set_name__(self, owner, name):
        self.name = name

    def __get__(self, obj, objtype=None):
        if obj is None:
            return self
        val = self.func(obj)
        setattr(obj, self.name, val)
        return val


class Circle:
    def __init__(self, r):
        self.r = r

    @LazyProp
    def area(self):
        return round(3.14 * self.r**2, 2)


c = Circle(10)
print(c.area, c.area)


def logged(func):
    def wrapper(self, *a, **k):
        return f"log:{func(self, *a, **k)}"

    return wrapper


class Svc:
    def __init__(self, n):
        self.n = n

    @logged
    def run(self):
        return self.n * 2


print(Svc(5).run())


import functools


def trace(func):
    @functools.wraps(func)
    def inner(self, x):
        return func(self, x) + 1

    return inner


class Calc:
    @trace
    def add_one(self, x):
        return x


print(Calc().add_one(10))


def repeat(times):
    def deco(func):
        def wrapper(self):
            return [func(self) for _ in range(times)]

        return wrapper

    return deco


class Rep:
    def __init__(self, v):
        self.v = v

    @repeat(3)
    def get(self):
        return self.v


print(Rep("x").get())


class Cached:
    def __init__(self, f):
        self.f = f
        self.cache = {}

    def __get__(self, obj, objtype=None):
        if obj is None:
            return self
        if id(obj) not in self.cache:
            self.cache[id(obj)] = self.f(obj)
        return self.cache[id(obj)]


class Data:
    def __init__(self, x):
        self.x = x

    @Cached
    def double(self):
        return self.x * 2


d = Data(21)
print(d.double, d.double)
