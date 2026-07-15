from decimal import Decimal
from fractions import Fraction
print(repr(Decimal('1.5')))
print(repr(Decimal('-0.0')))
print(repr(Decimal('100')))
print(repr(Decimal('1E+2')))
print(repr(Fraction(3, 2)))
print(repr(Fraction(-1, 4)))
print(repr(Fraction(5)))
print([Decimal('1.5'), Decimal('2.5')])
print({'a': Fraction(1, 3)})
print((Decimal('1'),))
