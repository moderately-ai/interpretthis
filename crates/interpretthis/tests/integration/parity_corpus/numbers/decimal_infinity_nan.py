from decimal import Decimal as D, InvalidOperation

# Construction and repr of the special values.
print(D("inf"), D("-inf"), D("Infinity"), D("-Infinity"))
print(D("nan"), D("NaN"))
print(repr(D("inf")), repr(D("-inf")), repr(D("nan")))

# Predicates.
print(D("inf").is_infinite(), D("inf").is_nan(), D("inf").is_finite())
print(D("nan").is_nan(), D("nan").is_finite())
print(D("5").is_finite(), D("5").is_infinite(), D("5").is_nan())
print(D("inf").is_signed(), D("-inf").is_signed())

# Arithmetic that stays valid.
print(D("inf") + D("1"))
print(D("inf") + D("inf"))
print(D("-inf") + D("-inf"))
print(D("inf") - D("-inf"))
print(D("inf") * D("2"))
print(D("inf") * D("-2"))
print(D("-inf") * D("inf"))
print(D("inf") / D("2"))
print(D("nan") + D("1"))
print(D("nan") * D("5"))

# A finite dividend over an infinity is a signed zero pinned to the context's
# Etiny exponent (0E-1000026 for the default context), not a bare 0.
print(repr(D("1") / D("inf")))
print(repr(D("-1") / D("inf")))
print(repr(D("1") / D("-inf")))
print(repr(D("-1") / D("-inf")))
print(repr(D("100") / D("inf")))


# Operations that create a NaN from non-NaN operands trap InvalidOperation.
def trap(fn):
    try:
        fn()
        return "no-trap"
    except InvalidOperation:
        return "InvalidOperation"


print(trap(lambda: D("inf") - D("inf")))
print(trap(lambda: D("inf") + D("-inf")))
print(trap(lambda: D("inf") * D("0")))
print(trap(lambda: D("inf") / D("inf")))

# Comparison.
print(D("inf") == D("inf"), D("inf") == D("-inf"), D("nan") == D("nan"))
print(D("nan") != D("nan"))
print(D("inf") > D("1000000"), D("-inf") < D("-1000000"))
print(D("5") < D("inf"), D("5") > D("-inf"))
print(sorted([D("1"), D("-inf"), D("inf"), D("0")]))

# Unary and abs.
print(-D("inf"), -D("-inf"), -D("nan"))
print(abs(D("-inf")), abs(D("inf")), abs(D("nan")))
print(bool(D("inf")), bool(D("nan")))

# as_tuple uses a string exponent for special values.
print(D("inf").as_tuple())
print(D("-inf").as_tuple())
print(D("nan").as_tuple())
print(D("2").as_tuple())
