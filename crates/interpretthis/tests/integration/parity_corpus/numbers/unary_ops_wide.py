# Unary -, +, ~ must handle every numeric type, not just i64-range int/float.
# Regression: -, +, ~ had arms only for Int/Float/Bool, so `-(10**20)` raised a
# bogus "bad operand type for unary -: 'int'", and negating i64::MIN wrapped
# (release) back to itself. ~ went through a to_int that raised OverflowError on
# any big int.
from decimal import Decimal
from fractions import Fraction

# Negation across the i64 boundary.
big = 10**20
print(-big)
print(-(-big))
print(-99999999999999999999999999999999)

# i64::MIN, reachable by arithmetic; negating it must promote, not wrap.
m = -9223372036854775807 - 1
print(m)
print(-m)

# Unary plus is identity on every numeric type.
print(+big)
print(+Decimal("1.5"))

# Decimal / Fraction negation.
print(-Decimal("1.5"))
print(-Decimal("-2.25"))
print(-Fraction(3, 4))
print(-Fraction(-1, 2))

# Bitwise invert on a big int: ~x == -x - 1.
print(~big)
print(~(2**70))
print(~5)
