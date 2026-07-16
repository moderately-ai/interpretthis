# Numeric dunder methods called explicitly compute the operation, and a real
# failure of an applicable operation (ZeroDivisionError) propagates rather than
# collapsing to AttributeError.

print((5).__add__(3), (10).__sub__(4), (5).__mul__(3))
print((7).__mod__(3), (5).__floordiv__(2), (2).__pow__(3))
print((5.0).__mul__(2.0), (7.0).__truediv__(2.0))
print((12).__and__(10), (12).__or__(3), (12).__xor__(10))
print((1).__lshift__(4), (256).__rshift__(2))


def show(fn):
    try:
        fn()
    except Exception as e:
        print(type(e).__name__, "::", e)


show(lambda: (10).__floordiv__(0))
show(lambda: (2).__truediv__(0))
show(lambda: (10).__mod__(0))
show(lambda: (1.0).__truediv__(0.0))
show(lambda: (1.0).__floordiv__(0.0))
