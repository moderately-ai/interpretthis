# Pins: child inherits parent __slots__ allowlist.
class A:
    __slots__ = ("x",)
    def __init__(self, x):
        self.x = x

class B(A):
    __slots__ = ("y",)
    def __init__(self, x, y):
        self.x = x
        self.y = y

b = B(1, 2)
print(b.x, b.y)
try:
    b.z = 3
    print("no-error")
except AttributeError:
    print("attr-error")
