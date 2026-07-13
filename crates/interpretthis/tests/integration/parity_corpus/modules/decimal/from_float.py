from decimal import Decimal

d = Decimal.from_float(0.5)
print(d)
# exact binary expansion of 0.1 starts with 0.10000000...
d2 = Decimal.from_float(0.1)
print(str(d2).startswith("0.10000000"))
