# Pins: `except (A, B):` catches either type.
try:
    raise TypeError("oops")
except (ValueError, TypeError):
    result = "caught"
print(result)
