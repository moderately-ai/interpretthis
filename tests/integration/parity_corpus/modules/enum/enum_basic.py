# Basic Enum subclass — class body assignments become enum members.
# Pins CPython semantics: members are wrapped in EnumMember objects
# with a custom repr (`Color.RED`), name/value attributes, and
# identity-based equality for plain Enum / value-based equality for
# IntEnum.
from enum import Enum, IntEnum

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

# Wrapped-member repr.
print(Color.RED)
print(Color.GREEN)
print(Color.BLUE)

# .name / .value attribute access.
print(Color.RED.name)
print(Color.RED.value)

# Plain Enum equality: NOT equal to raw literal (identity-based).
print(Color.RED == 1)
print(Color.RED == Color.RED)
print(Color.RED == Color.GREEN)

class Priority(IntEnum):
    LOW = 1
    MEDIUM = 5
    HIGH = 10

# IntEnum: members behave as ints.
print(Priority.LOW + Priority.HIGH)
print(Priority.MEDIUM < Priority.HIGH)
print(Priority.LOW == 1)
print(Priority.HIGH > Priority.MEDIUM)
print(Priority.LOW.name)
print(Priority.LOW.value)
