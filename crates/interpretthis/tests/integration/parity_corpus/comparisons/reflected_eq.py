# When the left operand's __eq__ is absent or returns NotImplemented, CPython
# tries the right operand's reflected __eq__ (== / != are symmetric). So
# `5 == Instance` gets a turn at `Instance.__eq__(5)`.
class Reflected:
    def __init__(self, v):
        self.v = v

    def __eq__(self, o):
        return self.v == (o.v if isinstance(o, Reflected) else o)


print(Reflected(5) == 5, 5 == Reflected(5))
print(Reflected(5) != 5, 5 != Reflected(5))
print(Reflected(5) == 6, 6 == Reflected(5))
print(Reflected(5) != 6, 6 != Reflected(5))


# The forward operand still wins when it returns a real (non-NotImplemented)
# result; the reflected slot is only a fallback.
class Forward:
    def __eq__(self, o):
        return True


print(Forward() == 1, 1 == Forward())


# Two instances: left's __eq__ returning NotImplemented falls back to the
# right's __eq__.
class OnlyMine:
    def __init__(self, v):
        self.v = v

    def __eq__(self, o):
        if isinstance(o, OnlyMine):
            return self.v == o.v
        return NotImplemented


print(OnlyMine(1) == OnlyMine(1), OnlyMine(1) == OnlyMine(2))
print(OnlyMine(1) == "x", "x" == OnlyMine(1))

# reflected __eq__ also drives `in` (list membership uses ==).
print(5 in [Reflected(5), Reflected(6)])
print(Reflected(5) in [1, 2, 5])
