# `is` is object identity. Regression: two contradictory implementations — the
# async path knew only `None is None`, so `x is x` was False for every reference
# type; the sync numeric path used value-equality, so `1 is 1` was True. Unify:
# Arc-backed reference types (list, instance, function) use true identity;
# immutable value types fall back to equality (matching CPython's small-int /
# short-string caching for the stable cases pinned here).

# Reference types: aliases are identical, distinct objects are not.
x = [1, 2]
print(x is x)
y = x
print(x is y)
print([1, 2] is [1, 2])

# None / singletons
print(None is None)
print(None is not None)

# Small ints and short strings (CPython caches these; equality-fallback agrees).
print(1 is 1)
print(1 is not 2)
s = "abc"
print(s is s)

# Functions and instances carry identity too.
def f():
    return 1

print(f is f)
g = f
print(f is g)

class C:
    pass

c = C()
print(c is c)
d = C()
print(c is d)
