# Iterating an enum class yields its members in definition order.
from enum import Enum


class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3


print([c.name for c in Color])
print([c.value for c in Color])
print(list(Color)[0].name)


# Definition order is preserved even when not alphabetical.
class Priority(Enum):
    HIGH = 3
    LOW = 1
    MEDIUM = 2


print([p.name for p in Priority])
print([p.value for p in Priority])


from enum import IntEnum


class Size(IntEnum):
    SMALL = 1
    LARGE = 100


print([s.name for s in Size])
print(sum(Size))
