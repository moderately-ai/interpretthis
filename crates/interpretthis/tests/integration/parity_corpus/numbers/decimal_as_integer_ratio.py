# Decimal.as_integer_ratio returns a plain (numerator, denominator) int tuple
# in lowest terms, not a Fraction.

from decimal import Decimal

print(Decimal("3.14").as_integer_ratio())
print(Decimal("10").as_integer_ratio())
print(Decimal("0.5").as_integer_ratio())
print(Decimal("-2.5").as_integer_ratio())
print(Decimal("100").as_integer_ratio())
print(Decimal("0").as_integer_ratio())
print(Decimal("1.000").as_integer_ratio())
r = Decimal("3.14").as_integer_ratio()
print(type(r).__name__, type(r[0]).__name__, r[0], r[1])
