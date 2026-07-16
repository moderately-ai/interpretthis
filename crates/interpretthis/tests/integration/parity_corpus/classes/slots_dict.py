# A class listing __dict__ in __slots__ keeps a real instance dict, so arbitrary
# attributes are allowed while the declared slots still surface via __slots__.
class WithDict:
    __slots__ = ("x", "__dict__")


w = WithDict()
w.x = 1
w.anything = 2
w.more = 3
print(w.x, w.anything, w.more)
print("__dict__" in WithDict.__slots__)


# Without __dict__ the restriction still applies.
class Strict:
    __slots__ = ("a",)


s = Strict()
s.a = 10
print(s.a)
try:
    s.b = 20
except AttributeError as e:
    print("AttributeError:", e)
