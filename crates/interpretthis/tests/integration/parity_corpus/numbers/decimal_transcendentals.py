from decimal import Decimal
print(Decimal(10).ln())
print(Decimal("2.5").ln())
print(Decimal(2).exp())
print(Decimal("0.5").exp())
print(Decimal(1000).log10())
print(Decimal("0.001").log10())
print(Decimal(100).log10())
print(Decimal(1).ln())
print(Decimal("1.5").log10())
try:
    Decimal(-5).ln()
except Exception as e:
    print(type(e).__name__)
print(Decimal(0).exp())
print(Decimal("1.00").ln())
print(Decimal("10").log10(), Decimal("1").log10())
print(Decimal("1000000").log10())
