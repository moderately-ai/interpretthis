def level1():
    raise ValueError("original error")
def level2():
    try:
        level1()
    except ValueError as e:
        raise RuntimeError("processing failed") from e
try:
    level2()
except RuntimeError as e:
    print(str(e))
    print(str(e.__cause__))
    print(type(e.__cause__).__name__)
try:
    try:
        1 / 0
    except ZeroDivisionError:
        raise ValueError("converted")
except ValueError as e:
    print(str(e))
    print(type(e.__context__).__name__)
def suppress_context():
    try:
        raise KeyError("k")
    except KeyError:
        raise RuntimeError("new") from None
try:
    suppress_context()
except RuntimeError as e:
    print(str(e), e.__cause__)
