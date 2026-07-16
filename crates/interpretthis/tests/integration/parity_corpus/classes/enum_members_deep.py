from enum import Enum, IntEnum, Flag, IntFlag, auto
class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3
print(Color.RED, Color.RED.name, Color.RED.value)
print(list(Color))
print(Color(2), Color["BLUE"])
print(Color.RED == Color.RED, Color.RED == Color.GREEN)
print(Color.RED is Color.RED)
for c in Color:
    print(c.name, c.value)
class Priority(IntEnum):
    LOW = 1
    MEDIUM = 2
    HIGH = 3
print(Priority.HIGH > Priority.LOW)
print(Priority.HIGH + 1)
print(int(Priority.MEDIUM))
print(sorted([Priority.HIGH, Priority.LOW, Priority.MEDIUM]))
class AutoEnum(Enum):
    A = auto()
    B = auto()
    C = auto()
print([e.value for e in AutoEnum])
class Perm(Flag):
    R = 4
    W = 2
    X = 1
print((Perm.R | Perm.W).value)
print(Perm.R in (Perm.R | Perm.W))
class FilePerm(IntFlag):
    READ = 4
    WRITE = 2
    EXECUTE = 1
combo = FilePerm.READ | FilePerm.WRITE
print(combo.value)
print(FilePerm.READ & combo == FilePerm.READ)
print(len(Color))
print(Color.RED.name, Color.RED.value)
class Status(Enum):
    ACTIVE = "active"
    INACTIVE = "inactive"
print(Status.ACTIVE.value)
print(Status("active"))
print([s.value for s in Status])
class Weekday(Enum):
    MON = 1
    TUE = 2
    def is_start(self):
        return self == Weekday.MON
print(Weekday.MON.is_start(), Weekday.TUE.is_start())
print(Color.RED != Color.BLUE)
d = {Color.RED: "stop", Color.GREEN: "go"}
print(d[Color.RED])
print(Color.RED in Color)
print(type(Color.RED).__name__)
print(repr(Color.RED))
print(str(Color.RED))
