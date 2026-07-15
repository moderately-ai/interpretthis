try:
    try:
        1 / 0
    except ZeroDivisionError as e:
        raise ValueError("wrapped") from e
except ValueError as e:
    print(type(e.__cause__).__name__ if e.__cause__ else "none")
    print(str(e))
try:
    raise KeyError("k")
except LookupError:
    print("lookup caught keyerror")
try:
    {}["x"]
except Exception as e:
    print(isinstance(e, KeyError), isinstance(e, LookupError))
