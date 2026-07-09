# CPython evaluates each operand in a chained comparison at most
# once and short-circuits on the first False — `a < b < c` is
# `a < b and b < c`. This probe verifies the booleans of several
# chains plus an exception-based short-circuit observable: a False
# first comparison must NOT evaluate the second, which would
# otherwise raise.
print(1 < 5 < 10)
print(1 < 5 < 3)
print(10 < 5 < 100)

class Raiser:
    def __lt__(self, other):
        raise RuntimeError("first compare raised")
try:
    _ = Raiser() < 1 < 2
except RuntimeError as e:
    print(f"RuntimeError: {e}")

# A False second comparison must not raise on the third (chains
# short-circuit on the first False).
class ThirdRaiser:
    def __lt__(self, other):
        raise RuntimeError("third compare raised")
# 1 < 5 is True, 5 < 0 is False, so chain stops at the second.
print(1 < 5 < 0 < ThirdRaiser())
