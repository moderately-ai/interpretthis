from decimal import Decimal
from fractions import Fraction
print(hash(Decimal('2')) == hash(2))
print(hash(Fraction(4, 2)) == hash(2))
print(len({Decimal('2'), 2}))
print(len({Fraction(4, 2), 2}))
print(len({Decimal('1.5'), Decimal('1.50')}))
print(len({Fraction(1, 2), Fraction(2, 4)}))
d = {Decimal('1.5'): 'a'}
d[Decimal('1.50')] = 'b'
print(d)
print(Decimal('2') in {2: 'x'})
print(2 in {Decimal('2')})
s = {Decimal('3.14'), Fraction(1, 3), Decimal('3.14')}
print(len(s))
print(sorted({Fraction(1, 2): 1, Fraction(3, 4): 2}.keys()))
counts = {}
for x in [Decimal('1.1'), Decimal('1.1'), Decimal('2.2')]:
    counts[x] = counts.get(x, 0) + 1
print(counts)
print(hash(Decimal('1.5')) == hash(1.5))
print(hash(Fraction(3, 2)) == hash(1.5))
print(hash(Decimal('1.5')) == hash(Fraction(3, 2)))
print(hash(Decimal('-0.0')) == hash(0))
print(hash(Fraction(-7, 4)) == hash(Decimal('-1.75')))
print(hash(Decimal('100')) == hash(100))
print(hash(Fraction(10, 5)) == hash(2))
