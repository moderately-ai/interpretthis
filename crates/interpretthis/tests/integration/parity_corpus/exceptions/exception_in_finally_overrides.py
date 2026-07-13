# Pins: an exception raised inside `finally` replaces (overrides) the
# original raised inside `try`. The outer except sees TypeError, not
# ValueError.
try:
    try:
        raise ValueError("original")
    finally:
        raise TypeError("from_finally")
except TypeError:
    result = "caught_type_error"
except ValueError:
    result = "caught_value_error"
print(result)
