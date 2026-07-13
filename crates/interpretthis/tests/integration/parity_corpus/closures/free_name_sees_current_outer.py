# A free name in an inner function that resolves to a module global
# must read the LIVE module value at call time, not a def-time
# snapshot. CPython's LEGB resolves `LOAD_GLOBAL x` against the
# module dict at every call; our prior closure-overlay model
# replayed the def-time value.
x = 1
def inner():
    return x

x = 2
print(inner())
