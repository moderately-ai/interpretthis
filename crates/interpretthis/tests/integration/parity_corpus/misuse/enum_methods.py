from enum import Enum, IntEnum, auto
class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3
    def describe(self):
        return f"{self.name} = {self.value}"
print(Color.RED.describe())
print(Color.RED.name, Color.RED.value)
print(list(Color))
print(Color(2))
print(Color["BLUE"])
print(Color.RED == Color.RED)
print(Color.RED == Color.GREEN)
class Priority(IntEnum):
    LOW = 1
    HIGH = 10
print(Priority.LOW < Priority.HIGH)
print(Priority.HIGH + 5)
class Auto(Enum):
    A = auto()
    B = auto()
    C = auto()
print([m.value for m in Auto])
