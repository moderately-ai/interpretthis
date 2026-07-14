# object() constructs a fresh, identity-bearing base instance — the common
# sentinel idiom. Regression: object() raised NameError even though `object`
# resolved as a base class and in isinstance/issubclass.
a = object()
b = object()
print(a is a, a is b)          # True False — distinct identities
print(a == a, a == b)          # True False — identity equality
print(type(a).__name__)        # object
print(isinstance(a, object))   # True
print(isinstance(5, object))   # True — everything is an object
print(hash(a) == hash(a))      # stable per-identity hash

# Usable as a dict key / set member via identity.
d = {a: 1, b: 2}
print(len(d))                  # 2 — distinct keys
print(len({a, b, a}))          # 2

# The sentinel pattern: distinguishable from any real value, incl. None.
_MISSING = object()
print(_MISSING is None, _MISSING is _MISSING)

# object() takes no arguments.
try:
    object(1)
except TypeError:
    print("TypeError")
