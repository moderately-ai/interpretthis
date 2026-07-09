# Pins: contextlib.nullcontext and suppress.
from contextlib import nullcontext, suppress

with nullcontext(42) as x:
    print(x)

with nullcontext() as y:
    print(y)

with suppress(ValueError):
    raise ValueError("hidden")
print("after-suppress")

with suppress(TypeError):
    raise ValueError("not-hidden")
print("should-not-print")
