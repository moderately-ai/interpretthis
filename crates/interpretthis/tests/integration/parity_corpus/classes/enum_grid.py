# Pins: Enum / IntEnum basics — member access, .name/.value,
# construction from value, comparison, IntEnum arithmetic.
from enum import Enum, IntEnum

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

print(Color.RED)
print(Color.RED.name)
print(Color.RED.value)
print(Color(1))
print(Color.RED == Color.RED)

class Priority(IntEnum):
    LOW = 1
    HIGH = 2

print(Priority.HIGH > Priority.LOW)
print(Priority.HIGH + 1)
