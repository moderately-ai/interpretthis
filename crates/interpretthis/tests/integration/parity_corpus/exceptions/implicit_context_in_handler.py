# Pins: when an exception is raised inside an except handler,
# CPython auto-sets `__context__` to the caught exception.
# Common pattern when error logging frameworks need to know
# the chain even without explicit `raise X from Y`.
try:
    try:
        raise ValueError('A')
    except ValueError:
        raise TypeError('B')
except TypeError as t:
    print(type(t.__context__).__name__)
    print(str(t.__context__))
