# Ordering comparisons are legal and exact across the numeric tower, just like
# equality: Decimal/Fraction compare against float and each other by exact value.
from decimal import Decimal
from fractions import Fraction

print(Decimal(3) < 3.5, Decimal(3) > 2.5, Decimal("2.5") <= 2.5, Decimal(3) >= 3.0)
print(Fraction(1, 2) < 0.6, Fraction(1, 2) > 0.4, Fraction(1, 2) <= 0.5, Fraction(3, 2) >= 1.5)
print(Decimal("0.5") < Fraction(2, 3), Fraction(2, 3) > Decimal("0.5"))
print(3.5 < Decimal(4), 0.6 > Fraction(1, 2))

# Sorting and min/max over a mixed-tower list must order by value.
print(sorted([3, Decimal("2.5"), Fraction(1, 2), 1.75, 2]))
print(min([Decimal(3), 2.5, Fraction(7, 3)]), max([Decimal(3), 2.5, Fraction(7, 3)]))

# Exactness: Fraction(1, 3) is strictly less than the float nearest to 1/3
# only if that float rounds up; check a case with a clear exact answer.
print(Fraction(1, 3) < 0.3334, Fraction(1, 3) > 0.3333)
