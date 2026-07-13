# Pins: mutable default arguments are shared across calls — the
# canonical CPython foot-gun. The default list is evaluated once at
# function-def time and aliased into every invocation that omits it.
# Mutating it through one call affects every subsequent call.
#
# Requires shared-storage Value::List (D2): without Arc-share, each
# call would receive an independent clone of the default.
def f(x, lst=[]):
    lst.append(x)
    return lst

print(f(1))
print(f(2))
print(f(3))
