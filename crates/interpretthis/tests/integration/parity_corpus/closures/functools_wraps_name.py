# functools.wraps copies the wrapped function's __name__ onto the wrapper.
from functools import wraps


def logged(fn):
    @wraps(fn)
    def wrapper(*args, **kwargs):
        return fn(*args, **kwargs)
    return wrapper


@logged
def greet(name):
    return f"hi {name}"


print(greet("x"))
print(greet.__name__)
print(greet.__qualname__)


def deco(fn):
    @wraps(fn)
    def inner(*a):
        return fn(*a) * 2
    return inner


@deco
def base(n):
    return n + 1


print(base(5), base.__name__)
