from enum import IntEnum, IntFlag
class P(IntEnum):
    LOW = 1; HIGH = 10
print(int(P.LOW), float(P.HIGH))
print(f"{P.HIGH:d}", f"{P.HIGH:03d}", f"{P.HIGH:x}")
print(f"{P.LOW:.2f}", format(P.HIGH, "b"))
print(P.HIGH + 5, P.LOW * 3, abs(P.LOW))
print(P.HIGH % 3, P.HIGH // 3, P.LOW - P.HIGH)
print(hex(P.HIGH), bin(P.LOW), oct(P.HIGH))
print(P.HIGH > 5, P.LOW == 1)
class F(IntFlag):
    A = 1; B = 2
print(int(F.A | F.B), f"{F.A | F.B:d}")
# A plain Enum is NOT an int — these must raise, not coerce.
from enum import Enum
class Plain(Enum):
    X = 1
for op in ("int", "float", "abs", "fmt"):
    try:
        {"int": lambda: int(Plain.X), "float": lambda: float(Plain.X),
         "abs": lambda: abs(Plain.X), "fmt": lambda: format(Plain.X, "d")}[op]()
        print(op, "coerced")
    except (TypeError, ValueError) as e:
        print(op, type(e).__name__)
