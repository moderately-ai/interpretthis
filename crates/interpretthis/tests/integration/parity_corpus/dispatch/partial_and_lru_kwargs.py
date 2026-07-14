# functools.partial keeps its bound keywords, and lru_cache keys on keywords.
# Regression: partial dropped data.keywords entirely, and lru_cache keyed on
# positional args only, so `f(1, b=2)` and `f(1, b=3)` collided.
from functools import lru_cache, partial


def label(value, *, prefix, suffix=""):
    return f"{prefix}{value}{suffix}"


p = partial(label, prefix=">> ")
print(p(5))
print(p(5, suffix="!"))          # call-site kwarg adds
print(p(5, prefix="** "))        # call-site kwarg overrides the bound one


@lru_cache
def add(a, b=0):
    return a + b


print(add(1, b=2))
print(add(1, b=3))               # must NOT collide with the b=2 result
print(add(1))
print(add(1, b=2))               # cache hit, same as first
