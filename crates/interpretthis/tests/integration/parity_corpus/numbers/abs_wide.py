# abs() handles every numeric type. Regression: abs(Int) used i.abs(), which
# PANICS on i64::MIN, and BigInt/Decimal/Fraction fell through to a bogus
# TypeError.
from decimal import Decimal
from fractions import Fraction

print(abs(-5))
print(abs(5))
print(abs(-3.5))
print(abs(True))

# i64::MIN, reachable by arithmetic — abs must promote, not panic/wrap.
m = -9223372036854775807 - 1
print(abs(m))

# big ints
print(abs(-(10**30)))
print(abs(10**30))

# Decimal / Fraction
print(abs(Decimal("-1.5")))
print(abs(Fraction(-3, 4)))
