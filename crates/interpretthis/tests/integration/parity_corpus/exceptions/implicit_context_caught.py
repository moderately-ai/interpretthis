# PEP 3134: __context__ is set when an exception is raised (or arises) while
# another exception is being handled, even if the new one is caught by an
# inner handler and never escapes the outer one.
try:
    raise Exception("outer")
except Exception:
    try:
        raise Exception("inner")
    except Exception as e:
        print(str(e), type(e.__context__).__name__, str(e.__context__))

# Implicit exceptions (not via an explicit raise) get the context too.
try:
    raise ValueError("first")
except ValueError:
    try:
        1 / 0
    except ZeroDivisionError as e:
        print(type(e).__name__, type(e.__context__).__name__, str(e.__context__))

# No enclosing handler → __context__ is None.
try:
    raise KeyError("k")
except KeyError as e:
    print(e.__context__)

# An exception that escapes the handler still chains (existing behaviour).
def reraise():
    try:
        raise ValueError("a")
    except ValueError:
        raise RuntimeError("b")

try:
    reraise()
except RuntimeError as e:
    print(str(e), type(e.__context__).__name__)

# Explicit `from` sets __cause__; __context__ still reflects the handled one.
try:
    raise ValueError("cause-src")
except ValueError:
    try:
        raise TypeError("chained") from IndexError("explicit")
    except TypeError as e:
        print(type(e.__cause__).__name__, type(e.__context__).__name__)
