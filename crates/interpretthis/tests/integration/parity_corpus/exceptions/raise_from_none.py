# `raise X from None` suppresses the implicit __context__ chaining, leaving
# __cause__ as None (so tracebacks omit "During handling of ...").
try:
    try:
        1 / 0
    except ZeroDivisionError:
        raise ValueError("boom") from None
except ValueError as e:
    print(e.__cause__)
    print(str(e))


# `raise X from Y` sets __cause__ to Y.
try:
    try:
        1 / 0
    except ZeroDivisionError as z:
        raise ValueError("wrapped") from z
except ValueError as e:
    print(type(e.__cause__).__name__)
