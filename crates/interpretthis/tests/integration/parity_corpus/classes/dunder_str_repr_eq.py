# Pins: user-class __str__ and __repr__ slots dispatch from str(),
# repr(), print(), and f-string conversions (`{x}`, `{x!s}`, `{x!r}`).
# Heavy customer pattern in domain-model code emitted by agents.
class Box:
    def __init__(self, val):
        self.val = val
    def __str__(self):
        return f"Box[{self.val}]"
    def __repr__(self):
        return f"Box(val={self.val!r})"

a = Box(1)
print(str(a))
print(repr(a))
print(f"{a}")
print(f"{a!r}")
print([Box(1), Box(2), Box(3)])
