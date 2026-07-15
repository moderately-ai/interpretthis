def documented():
    """This is a docstring."""
    return 1
print(documented.__doc__)
def undocumented(): return 2
print(undocumented.__doc__)
from functools import wraps
def deco(func):
    @wraps(func)
    def w(*a, **k): return func(*a, **k)
    return w
@deco
def g():
    """g's doc"""
    return 1
print(g.__name__, g.__doc__)
print((lambda x: x).__doc__)
