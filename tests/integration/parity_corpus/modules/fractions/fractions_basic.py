# Basic Fraction — exact rational arithmetic.
#
# Pins CPython semantics: Fraction(num, denom) auto-simplifies, repr is
# `Fraction(num, denom)`, arithmetic stays rational, and mixed
# Fraction-int promotes to Fraction (the int side gets denom=1).
from fractions import Fraction

# Auto-simplification: 6/4 -> 3/2.
f = Fraction(6, 4)
print(f)
print(f.numerator)
print(f.denominator)

# Arithmetic stays rational (no float drift).
print(Fraction(1, 3) + Fraction(1, 6))
print(Fraction(1, 2) - Fraction(1, 4))
print(Fraction(2, 3) * Fraction(3, 4))
print(Fraction(1, 2) / Fraction(1, 4))

# Mixed Fraction + int promotes int to denom=1.
print(Fraction(1, 2) + 1)
print(Fraction(3, 4) * 2)

# Construction from a string parses "num/denom" and bare ints.
print(Fraction("3/7"))
print(Fraction("5"))

# Negative numerator normalises to the sign on the numerator.
print(Fraction(-3, 4))
print(Fraction(3, -4))

# Comparison with int / fraction.
print(Fraction(1, 2) < Fraction(2, 3))
print(Fraction(1, 2) == Fraction(2, 4))
print(Fraction(3, 2) > 1)
