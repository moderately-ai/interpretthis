try:
    raise ValueError("bad", 42)
except ValueError as e:
    print(e.args)
    print(str(e))
try:
    raise KeyError("k")
except KeyError as e:
    print(repr(e))
try:
    try:
        raise ValueError("inner")
    except ValueError as e:
        raise RuntimeError("outer") from e
except RuntimeError as e:
    print(e.__cause__)
    print(type(e.__cause__).__name__)
