# Pins: NotImplemented from __add__ triggers reflected __radd__.
class Left:
    def __add__(self, other):
        return NotImplemented

class Right:
    def __radd__(self, other):
        return "reflected"

print(Left() + Right())
print(NotImplemented)
