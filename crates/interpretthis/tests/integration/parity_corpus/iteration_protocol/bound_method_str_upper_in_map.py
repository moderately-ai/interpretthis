# Pins: `map(str.upper, items)` — unbound-method-of-type used as map fn.
# CPython: `str.upper` is a function object; calling it with a string as
# first arg invokes the method. Our model needs to either bind it
# inline or treat `str.upper` as a callable that takes self as positional.
print(list(map(str.upper, ['abc', 'def'])))
