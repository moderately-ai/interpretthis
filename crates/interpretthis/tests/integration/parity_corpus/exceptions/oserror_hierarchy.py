try:
    raise IndexError("idx")
except LookupError as e:
    print("lookup", type(e).__name__)
try:
    raise KeyError("k")
except LookupError:
    print("key is lookup")
try:
    raise ZeroDivisionError("zero")
except ArithmeticError:
    print("zero is arithmetic")
try:
    raise FileNotFoundError("nf")
except OSError:
    print("fnf is oserror")
for exc in [ValueError, TypeError, KeyError, RuntimeError]:
    print(exc.__name__, issubclass(exc, Exception))
try:
    raise UnicodeDecodeError("utf-8", b"", 0, 1, "bad")
except ValueError:
    print("unicode is value")
print(issubclass(BrokenPipeError, ConnectionError), issubclass(ConnectionError, OSError))
try:
    raise StopIteration(42)
except StopIteration as e:
    print(e.value)
