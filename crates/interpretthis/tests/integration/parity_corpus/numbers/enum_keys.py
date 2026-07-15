from enum import Enum, IntEnum, Flag
class Color(Enum):
    RED = 1; GREEN = 2; BLUE = 3
class P(IntEnum):
    LOW = 1; HIGH = 10
# plain Enum members as dict keys and set members
d = {Color.RED: "r", Color.GREEN: "g"}
print(d[Color.RED], d[Color.GREEN])
print(Color.RED in d, Color.BLUE in d)
s = {Color.RED, Color.GREEN, Color.RED}
print(len(s), Color.RED in s)
# IntEnum keys fold with equal ints (hash(P.HIGH)==hash(10), P.HIGH==10)
d2 = {P.HIGH: "hi"}
print(d2[10], 10 in d2, P.HIGH in d2)
d3 = {10: "ten"}
print(d3[P.HIGH])
print({P.LOW, 1} == {1})
# counting with enum keys
from collections import Counter
c = Counter([Color.RED, Color.RED, Color.GREEN])
print(c[Color.RED], c[Color.GREEN])
