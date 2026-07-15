# Exception chaining, args, __cause__/__context__, custom hierarchy.
try:
    try:
        1 / 0
    except ZeroDivisionError as e:
        raise ValueError("wrapped") from e
except ValueError as e:
    print(type(e).__name__, e.args, str(e))
    print(type(e.__cause__).__name__)

# implicit context: an exception raised while handling another chains it as
# __context__ (PEP 3134), even without an explicit `from`.
try:
    try:
        [][0]
    except IndexError:
        {}["missing"]
except KeyError as e:
    print(type(e.__context__).__name__)
    print(e.__cause__ is None, e.__suppress_context__)

# explicit `raise ... from` sets __cause__ and suppresses context display.
try:
    try:
        1 / 0
    except ZeroDivisionError as z:
        raise ValueError("v") from z
except ValueError as e:
    print(type(e.__cause__).__name__, e.__suppress_context__)

# custom exception with extra attributes
class MyError(Exception):
    def __init__(self, code, msg):
        super().__init__(msg)
        self.code = code

try:
    raise MyError(42, "custom failure")
except MyError as e:
    print(e.code, e.args, str(e))

# exception hierarchy and multiple except
for exc in [ValueError("v"), TypeError("t"), KeyError("k")]:
    try:
        raise exc
    except (ValueError, TypeError) as e:
        print("VT:", type(e).__name__)
    except Exception as e:
        print("other:", type(e).__name__)

# re-raise bare
def reraise():
    try:
        raise RuntimeError("original")
    except RuntimeError:
        raise

try:
    reraise()
except RuntimeError as e:
    print("reraised:", e)

# finally with return
def f():
    try:
        return "try"
    finally:
        print("finally runs")

print(f())
