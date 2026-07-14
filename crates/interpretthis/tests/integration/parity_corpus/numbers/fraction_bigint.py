# Fraction accepts and preserves numerators/denominators beyond i64 (its storage
# is arbitrary precision). Regression: the constructor rejected a BigInt with
# "numerator expects int, got 'int'".
from fractions import Fraction

f = Fraction(10**30, 3)
print(f)
print(f.numerator)
print(f.denominator)

g = Fraction(2**80)
print(g, g.numerator, g.denominator)

h = Fraction(10**30, 10**20)      # reduces to 10**10 / 1
print(h, h.numerator, h.denominator)

print(Fraction(10**25, 5) + Fraction(1, 5))
