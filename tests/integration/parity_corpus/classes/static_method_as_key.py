# Pins: `sorted(xs, key=Cls.staticmethod)` — class staticmethod passed
# as a key function. `Cls.method` returns a `__class_method__` sentinel
# string today; pass into key= and it errors "not callable".
class Cls:
    @staticmethod
    def neg(x):
        return -x
print(sorted([3, 1, 2], key=Cls.neg))
