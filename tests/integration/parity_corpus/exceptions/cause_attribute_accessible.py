# Pins: `raise X from Y` chains exceptions via __cause__ — the
# inner exception is accessible on .__cause__ for code that wants
# to inspect the chain (common in error-logging frameworks).
try:
    try:
        raise ValueError('inner')
    except ValueError as inner:
        raise TypeError('outer') from inner
except TypeError as e:
    print(str(e))
    print(type(e.__cause__).__name__)
    print(str(e.__cause__))
