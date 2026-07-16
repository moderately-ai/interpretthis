# reversed() on a user object: __reversed__ is called and its iterator returned
# directly; else the __len__ + __getitem__ sequence protocol is reverse-indexed
# (bounded by __len__, so a __getitem__ that never raises IndexError still
# terminates); an object with only __iter__ (or neither) is not reversible.
class HasReversed:
    def __reversed__(self):
        return iter(["z", "y", "x"])


class RevGen:
    def __reversed__(self):
        yield from [3, 2, 1]


class LenGetitem:
    def __len__(self):
        return 3

    def __getitem__(self, i):
        return i * 10


class OnlyIter:
    def __iter__(self):
        return iter([1, 2, 3])


class Nothing:
    pass


print(list(reversed(HasReversed())))
print(list(reversed(RevGen())))
print(list(reversed(LenGetitem())))
print(type(reversed(HasReversed())).__name__)
print(type(reversed(RevGen())).__name__)
print(type(reversed(LenGetitem())).__name__)
for cls in (OnlyIter, Nothing):
    try:
        list(reversed(cls()))
    except TypeError as e:
        print(cls.__name__, "TypeError:", e)
