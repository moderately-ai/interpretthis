# Rich-comparison and arithmetic dunders return NotImplemented (not a bool /
# not AttributeError) when the operand types are unrelated, following CPython's
# asymmetric per-type rule. Sequence concat/repeat dunders raise instead.


def r(v):
    return "NotImplemented" if v is NotImplemented else v


# __eq__ / __ne__: the numeric tower is asymmetric (int doesn't know float).
print(r((5).__eq__(5)), r((5).__eq__(6)), r((5).__eq__("x")), r((5).__eq__(5.0)))
print(r((5.0).__eq__(5)), r((5.0).__eq__("x")), r((1j).__eq__(1)), r((1j).__eq__(1.0)))
print(r("a".__eq__("a")), r("a".__eq__(5)), r(b"a".__eq__(bytearray(b"a"))))
print(r([1].__eq__([1])), r([1].__eq__((1,))), r((1,).__eq__([1])))
print(r({1}.__eq__(frozenset({1}))), r({1}.__eq__([1])), r(None.__eq__(5)))
print(r({1: 2}.__eq__({1: 2})), r({1: 2}.__eq__([1])), r((1).__ne__("x")))
print(r(True.__eq__(1)), r(True.__eq__("x")))


# Arithmetic dunders: numeric receivers return NotImplemented on a type mismatch.
print(r((5).__add__("x")), r((5.0).__mul__("x")), r((5).__sub__([])))
print(r((5).__add__(3)), r((2).__pow__(10)), r((10).__floordiv__(3)))


# Sequence concat/repeat dunders RAISE for the wrong operand type.
def show(fn):
    try:
        fn()
    except TypeError as e:
        print("TypeError:", e)


show(lambda: [1].__add__(5))
show(lambda: "a".__add__(5))
show(lambda: (1,).__add__(5))
show(lambda: b"a".__add__(5))
print([1].__add__([2]), "a".__mul__(3), [1].__mul__(3))
