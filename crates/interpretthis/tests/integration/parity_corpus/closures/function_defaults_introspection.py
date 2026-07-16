# __defaults__ is a tuple of the trailing positional parameters' defaults (or
# None), and __kwdefaults__ is a dict of keyword-only defaults (or None).
def f(a, b=10, c=20):
    return a + b + c


print(f.__defaults__)


def g(a, *, b, c=5):
    return a + b + c


print(g.__kwdefaults__)


def h():
    pass


print(h.__defaults__)
print(h.__kwdefaults__)


def only_kw(*, a=1, b=2):
    return a + b


print(only_kw.__kwdefaults__)
print(only_kw.__defaults__)


def mixed(x, y=1, *, z=2):
    return x


print(mixed.__defaults__, mixed.__kwdefaults__)


def with_ann(x: int = 5) -> str:
    return str(x)


print(with_ann.__defaults__)
print(hasattr(f, "__defaults__"), hasattr(f, "__kwdefaults__"))
