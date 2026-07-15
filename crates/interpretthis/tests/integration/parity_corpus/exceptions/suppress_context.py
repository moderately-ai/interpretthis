# __suppress_context__ is set by `raise X from Y` (including `from None`) and
# defaults to False for a plain raise or an implicitly-raised exception.
try:
    raise ValueError("a") from None
except ValueError as e:
    print(e.__cause__, e.__suppress_context__)
try:
    raise ValueError("x") from KeyError("y")
except ValueError as e:
    print(type(e.__cause__).__name__, e.__suppress_context__)
try:
    try:
        raise KeyError("k")
    except KeyError:
        raise ValueError("v")
except ValueError as e:
    print(type(e.__context__).__name__, e.__suppress_context__)
try:
    raise RuntimeError("plain")
except RuntimeError as e:
    print(e.__suppress_context__)
try:
    {}["missing"]
except KeyError as e:
    print(e.__suppress_context__)
