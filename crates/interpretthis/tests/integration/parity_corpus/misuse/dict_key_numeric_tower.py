# Dict keys unify across the numeric tower exactly like set members: equal
# values that hash equally are the SAME key, and the first-inserted key object
# is retained while a later equal key updates the value in place.
from decimal import Decimal
from fractions import Fraction

d = {3: "a"}
print(d[3.0], d[Decimal(3)], d[Fraction(3, 1)], d[True] if 1 == True else None)

d2 = {1: "x"}
d2[1.0] = "y"
d2[True] = "z"
print(d2, len(d2))

# Fraction/float keys collapse.
d3 = {Fraction(1, 2): "half"}
print(d3[0.5], d3[Fraction(2, 4)])
d3[0.5] = "HALF"
print(d3, len(d3))

# Mixed-tower keys: first object kind is kept as the stored key.
d4 = {}
d4[2] = "int"
d4[2.0] = "float"
d4[Decimal(2)] = "dec"
print(list(d4.items()), len(d4))

# get/in across the tower.
print(0.5 in d3, Decimal("0.5") in d3, Fraction(1, 2) in d3)
print({1: "a", 2: "b"}.get(1.0), {1: "a"}.get(Decimal(1)))
