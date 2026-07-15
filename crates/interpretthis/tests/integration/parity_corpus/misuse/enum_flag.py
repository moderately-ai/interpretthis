from enum import Flag, IntFlag, auto
class Perm(Flag):
    R = auto()
    W = auto()
    X = auto()
print((Perm.R | Perm.W).value)
print(Perm.R in (Perm.R | Perm.W))
class Color(IntFlag):
    RED = 1
    GREEN = 2
    BLUE = 4
print((Color.RED | Color.BLUE).value)
print(Color.RED & Color.RED)
print(bool(Color.RED & Color.GREEN))
print((Color.RED | Color.GREEN | Color.BLUE).value)
print(str(Perm.R))
print(str(Perm.R | Perm.X))
print(str(Color.RED))
print(str(Color.RED | Color.BLUE))
print((Color.RED | Color.GREEN) ^ Color.RED)
print(Color.GREEN in (Color.RED | Color.GREEN))
print(Color.BLUE in (Color.RED | Color.GREEN))
print(list(Perm))
print(Perm.R.name, Perm.R.value)
