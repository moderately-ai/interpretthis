import math
from fractions import Fraction
from decimal import Decimal
print(math.floor(Fraction(7,2)), math.ceil(Fraction(7,2)), math.trunc(Fraction(7,2)))
print(math.floor(Fraction(-7,2)), math.ceil(Fraction(-7,2)), math.trunc(Fraction(-7,2)))
print(math.floor(Decimal("2.7")), math.ceil(Decimal("2.1")), math.trunc(Decimal("-2.7")))
print(int(Fraction(7,2)), int(Decimal("3.9")))
print(abs(Fraction(-3,4)), abs(Decimal("-5.5")))
print(float(Decimal("1.5")), float(Fraction(3,4)))
print(Fraction(7,2).__round__(), Decimal("2.5").__round__())
print(math.gcd(12, 18), math.lcm(4, 6))
print((-7) // 2, (-7) % 2)
print(7.5 // 2, 7.5 % 2)
print(divmod(-7, 2))
